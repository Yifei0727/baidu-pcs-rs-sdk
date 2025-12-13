use log::{debug, info};
use md5::{Digest, Md5};
use reqwest::{Body, Client};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::baidu_pcs_sdk::pcs::HttpMethod::{Get, Post};
pub use crate::baidu_pcs_sdk::{
    AppError, AppErrorType, BaiduPcsApp, PcsApiError, PcsCreateFolderResult, PcsDiskQuota,
    PcsFileListResult, PcsFileMetaResult, PcsFileSearchResult, PcsFileSlicePrepareResult,
    PcsFileUploadResult, PcsUserInfo, UploadServerResult,
};

use crate::dns;
use futures::TryStreamExt;
use tokio_util::io::ReaderStream;

pub enum PcsUploadPolicy {
    /// 失败
    Fail,
    /// 重命名
    Rename,
    /// 覆盖
    Overwrite,
    /// 重命名
    NewCopy,
}

/// @see https://pan.baidu.com/union/doc/Cksg0s9ic
const PREFIX: &str = "https://pan.baidu.com";
// 根据文档和测试， 若api管理用 pan.baidu.com， 文件上传下载用 d.pcs.baidu.com
const PREFIX_FILE_SERVER: &str = "https://d.pcs.baidu.com";
/// 分片文件头部摘要大小 256KB
const HEADER_SLICE_SIZE: u64 = 256 * 1024;

/// 将文件进行切片后的文件信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PcsFileSliceInfo {
    /// 本地文件路径
    path: String,
    /// 本地文件大小
    size: u64,
    /// 本地文件MD5，32位小写
    content_md5: String,
    /// 文件校验段的MD5，32位小写，校验段对应文件前256KB
    slice_md5: String,
    /// 文件各分片md5数组的json串
    block_list: Vec<String>,
    /// 本地文件创建时间(精确到秒)
    ctime: i64,
    /// 本地文件修改时间(精确到秒)
    mtime: i64,
}

/// 百度网盘--网盘客户端
pub struct BaiduPcsClient {
    runtime: tokio::runtime::Runtime,
    pcs_app: BaiduPcsApp,
    client: Client,
    access_token: String,
    user_info: Option<PcsUserInfo>,
    disk_quota: Option<PcsDiskQuota>,
    /// 指定的 DNS 服务器（逗号分隔），用于网络请求解析域名
    dns: Option<String>,
}

fn get_file_block_list(
    user_info: &PcsUserInfo,
    file_path: &str,
) -> Result<PcsFileSliceInfo, AppError> {
    let mut file = File::open(file_path)?;
    let file_meta = file.metadata()?;
    let file_size = file_meta.len();
    let slice_size = user_info.get_user_block_slice_size();
    let parts = if slice_size == 0 {
        0
    } else {
        file_size.div_ceil(slice_size)
    };

    // slice_md5: 文件前 256KB
    let slice_md5 = {
        let slice_len = HEADER_SLICE_SIZE.min(file_size) as usize;
        let mut buffer = vec![0u8; slice_len];
        file.read_exact(&mut buffer)?;
        let mut hasher = Md5::new();
        Digest::update(&mut hasher, &buffer);
        hex::encode(hasher.finalize())
    };

    file.rewind()?;

    // content_md5 与每块 md5
    let mut file_hasher = Md5::new();
    let mut block_list = Vec::with_capacity(parts as usize);
    for i in 0..parts {
        let is_last = i == parts - 1;
        let this_len = if is_last {
            let rem = file_size % slice_size;
            if rem == 0 {
                slice_size
            } else {
                rem
            }
        } else {
            slice_size
        } as usize;
        let mut buffer = vec![0u8; this_len];
        file.read_exact(&mut buffer)?;
        Digest::update(&mut file_hasher, &buffer);
        let mut part_hasher = Md5::new();
        Digest::update(&mut part_hasher, &buffer);
        block_list.push(hex::encode(part_hasher.finalize()));
    }
    let content_md5 = hex::encode(file_hasher.finalize());

    Ok(PcsFileSliceInfo {
        path: file_path.to_string(),
        size: file_size,
        content_md5,
        slice_md5,
        block_list,
        ctime: file_meta
            .created()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        mtime: file_meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
    })
}

fn if_rest_ok_then_get_else_err<R>(text: String) -> Result<R, AppError>
where
    R: DeserializeOwned,
{
    let status: PcsApiError = serde_json::from_str(text.as_str()).unwrap_or_else(|_| PcsApiError {
        errno: i32::MIN,
        err_msg: None,
        request_id: None,
        raw: text.clone(),
    });
    match status.errno {
        0 => {
            let resp: R = serde_json::from_str(text.as_str())?;
            Ok(resp)
        }
        _ => Err(status.into()),
    }
}

enum HttpMethod {
    Get,
    Post,
}

#[derive(Serialize, Debug, Clone)]
pub struct ProgressInfo {
    /// 总字节数
    pub total_bytes: u64,
    /// 已上传字节数
    pub uploaded_bytes: u64,
    /// 当前分片序号，从0开始
    pub current_part: u32,
    /// 当前分片字节数
    pub current_part_bytes: u64,
}

impl Display for ProgressInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ProgressInfo {{ total_bytes: {}, uploaded_bytes: {}, current_part: {}, current_part_bytes: {} }}",
            self.total_bytes, self.uploaded_bytes, self.current_part, self.current_part_bytes
        )
    }
}

impl BaiduPcsClient {
    pub fn new(access_token: &str, app: BaiduPcsApp) -> Self {
        Self::new_with_dns(access_token, app, None)
    }

    pub fn new_with_dns(access_token: &str, app: BaiduPcsApp, dns: Option<&str>) -> Self {
        let builder = Client::builder();
        // 应用用户代理与通用头
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("User-Agent", "pan.baidu.com".parse().unwrap());
        headers.insert(
            "Content-Type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );
        let builder = crate::dns::use_custom_dns_if_present(builder, dns);

        headers.insert("Accept", "application/json".parse().unwrap());
        Self {
            pcs_app: app,
            client: builder.default_headers(headers).build().unwrap(),
            access_token: access_token.to_string(),
            runtime: tokio::runtime::Runtime::new().unwrap(),
            user_info: None,
            disk_quota: None,
            dns: dns.map(|s| s.to_string()),
        }
    }

    pub fn ware(&mut self) -> Result<(), AppError> {
        self.user_info = Some(self.get_user_info()?);
        self.disk_quota = Some(self.get_user_quota(false, false)?);
        Ok(())
    }

    pub fn get_apps_path(&self) -> PathBuf {
        PathBuf::from("/apps").join(self.pcs_app.get_app_name())
    }

    fn request<T, P, R>(
        &self,
        m: HttpMethod,
        path: &str,
        params: T,
        payload: Option<P>,
    ) -> Result<R, AppError>
    where
        T: Serialize,
        P: Serialize,
        R: DeserializeOwned,
    {
        let url = format!("{}{}", PREFIX, path);
        self._request(url, m, params, payload)
    }

    fn _request<T, P, R>(
        &self,
        url: String,
        m: HttpMethod,
        params: T,
        payload: Option<P>,
    ) -> Result<R, AppError>
    where
        T: Serialize,
        P: Serialize,
        R: DeserializeOwned,
    {
        debug!(
            "_request {} {} {} {}",
            url,
            match m {
                Get => "GET",
                Post => "POST",
            },
            serde_json::to_string(&params).unwrap_or_default(),
            if payload.is_some() {
                "with payload"
            } else {
                "no payload"
            }
        );
        let fetch = async {
            match m {
                Get => self.client.get(url.as_str()),
                Post => {
                    let chain = self.client.post(url.as_str());
                    match payload {
                        Some(p) => chain.form(&p),
                        None => chain,
                    }
                }
            }
            .query(&params)
            .query(&[("access_token", self.access_token.as_str())])
            .send()
            .await
            .unwrap()
            .text()
            .await
        };
        let text = self
            .runtime
            .block_on(fetch)
            .map_err(|e| AppError::new(AppErrorType::Network, e.to_string().as_str(), None))?;
        debug!("_request response text: {}", text);
        if_rest_ok_then_get_else_err(text)
    }

    /// 获取用户信息
    ///
    /// 本接口用于获取用户的基本信息，包括账号、头像地址、会员类型等。
    pub fn get_user_info(&self) -> Result<PcsUserInfo, AppError> {
        #[derive(Serialize)]
        struct Params<'a> {
            /// method 本接口固定为uinfo
            method: &'a str,
        }
        const PATH: &str = "/rest/2.0/xpan/nas";
        const PARAMS: Params = Params { method: "uinfo" };
        self.request(Get, PATH, PARAMS, None::<()>)
    }

    /// 获取网盘容量信息
    ///
    /// 本接口用于获取用户的网盘空间的使用情况，包括总空间大小，已用空间和剩余可用空间情况。
    /// # Arguments
    /// * `check_free` - 是否检查免费信息，0为不查，1为查，默认为0
    /// * `check_expire` - 是否检查过期信息，0为不查，1为查，默认为0
    pub fn get_user_quota(
        &self,
        check_free: bool,
        check_expire: bool,
    ) -> Result<PcsDiskQuota, AppError> {
        const PATH: &str = "/api/quota";
        #[derive(Serialize)]
        struct Params {
            /// `checkfree` 是否检查免费信息，0为不查，1为查，默认为0
            #[serde(alias = "checkfree")]
            check_free: u8,
            /// `checkexpire` 是否检查过期信息，0为不查，1为查，默认为0
            #[serde(alias = "checkexpire")]
            check_expire: u8,
        }
        self.request(
            Get,
            PATH,
            Params {
                check_free: if check_free { 1 } else { 0 },
                check_expire: if check_expire { 1 } else { 0 },
            },
            None::<()>,
        )
    }

    /// 创建文件夹
    /// 本接口用于创建文件夹。 https://pan.baidu.com/union/doc/6lbaqe1lw
    /// 对于已存在的目录
    pub fn create_folder(&self, path: &str) -> Result<PcsCreateFolderResult, AppError> {
        const PATH: &str = "/rest/2.0/xpan/file";
        #[derive(Serialize)]
        struct Params<'a> {
            /// 本接口固定为create
            method: &'a str,
        }
        const PARAMS: Params = Params { method: "create" };
        #[derive(Serialize)]
        struct FolderAttributes<'a> {
            /// 创建文件夹的绝对路径，需要urlencode
            path: String,
            /// 本接口固定为1
            isdir: &'a str,
            /// 文件命名策略，默认0
            // 0 为不重命名，返回冲突
            // 1 为只要path冲突即重命名
            // 2 为path冲突且block_list不同才重命名
            // 3 为覆盖，需要与预上传precreate接口中的rtype保持一致
            #[serde(skip_serializing_if = "Option::is_none")]
            rtype: Option<i32>,
            /// 客户端创建时间(精确到秒)，默认为当前时间戳
            #[serde(skip_serializing_if = "Option::is_none")]
            local_ctime: Option<i64>,
            /// 客户端修改时间(精确到秒)，默认为当前时间戳
            #[serde(skip_serializing_if = "Option::is_none")]
            local_mtime: Option<i64>,
            /// 上传方式
            /// - `1` 手动
            /// - `2` 批量上传
            /// - `3` 文件自动备份
            /// - `4` 相册自动备份
            /// - `5` 视频自动备份
            #[serde(skip_serializing_if = "Option::is_none")]
            mode: Option<i32>,
        }
        self.request(
            Post,
            PATH,
            PARAMS,
            Some(FolderAttributes {
                path: String::from(path),
                isdir: "1",
                rtype: None,
                local_ctime: None,
                local_mtime: None,
                mode: None,
            }),
        )
    }

    /// 删除文件或目录
    /// 本接口用于删除文件或目录。 https://pan.baidu.com/union/doc/mksg0s9l4
    /// # Arguments
    /// * `path` - 文件或目录的绝对路径
    /// * `async` - 是否异步删除，默认为0
    pub fn delete(
        &self,
        paths: &Vec<String>,
        is_async: Option<bool>,
    ) -> Result<crate::baidu_pcs_sdk::PcsFileTaskOperationResult, AppError> {
        const PATH: &str = "/rest/2.0/xpan/file";
        #[derive(Serialize)]
        struct Params<'a> {
            /// 本接口固定为`filemanager`
            method: &'a str,
            /// 文件操作参数，可实现文件复制、移动、重命名、删除，依次对应的参数值为：copy、move、rename、delete
            opera: &'a str,
        }
        #[derive(Serialize)]
        struct DeleteAttributes {
            /// 是否异步删除，0 同步，1 自适应，2 异步
            r#async: u8,
            #[serde(alias = "filelist")]
            file_list: String,
        }
        let files = DeleteAttributes {
            r#async: match is_async {
                Some(false) => 2,
                Some(true) => 0,
                None => 1,
            },
            file_list: serde_json::to_string(paths)?,
        };
        self.request(
            Post,
            PATH,
            Params {
                method: "filemanager",
                opera: "delete",
            },
            Some(files),
        )
    }

    /// 获取分片上传服务器
    ///https://pan.baidu.com/union/doc/Mlvw5hfnr
    pub(crate) fn get_upload_server(
        &self,
        task: &PcsFileSlicePrepareResult,
    ) -> Result<UploadServerResult, AppError> {
        const PATH: &str = "/rest/2.0/pcs/file";
        #[derive(Serialize)]
        struct Params<'a> {
            ///本接口固定为`locateupload`
            method: &'a str,
            ///应用ID，本接口固定为`250528`
            appid: &'a str,
            ///上传后使用的文件绝对路径，需要urlencode
            path: &'a str,
            ///上传ID
            #[serde(rename = "uploadid")]
            upload_id: &'a str,
            ///版本号，本接口固定为2.0
            upload_version: &'a str,
        }
        let url = format!("{}{}", PREFIX_FILE_SERVER, PATH);
        self._request(
            url,
            Get,
            Params {
                method: "locateupload",
                appid: "250528",
                path: task.path().as_str(),
                upload_id: task.upload_id().as_str(),
                upload_version: "2.0",
            },
            None::<()>,
        )
    }

    /// 列出目录文件
    /// 本接口用于列出指定目录下的文件和子目录信息。 https://pan.baidu.com/union/doc/mksg0s9l4
    pub fn list_dir(&self, path: &str) -> Result<PcsFileListResult, AppError> {
        const PATH: &str = "/rest/2.0/xpan/file";
        #[derive(Serialize)]
        struct Params<'a> {
            /// 本接口固定为`list`
            method: &'a str,
            /// 需要list的目录，以/开头的绝对路径, 默认为/
            // 路径包含中文时需要UrlEncode编码
            // 给出的示例的路径是/测试目录的UrlEncode编码
            dir: &'a str,
            /// 排序字段：默认为name；
            /// - `time` 表示先按文件类型排序，后按修改时间排序；
            /// - `name` 表示先按文件类型排序，后按文件名称排序；(注意，此处排序是按字符串排序的，如果用户有剧集排序需求，需要自行开发)
            /// - `size` 表示先按文件类型排序，后按文件大小排序。
            order: Option<String>,
            ///默认为升序，设置为1实现降序 （注：排序的对象是当前目录下所有文件，不是当前分页下的文件）
            desc: Option<i32>,
            ///起始位置，从0开始
            start: Option<u64>,
            ///查询数目，默认为1000，建议最大不超过1000
            limit: Option<u64>,
            ///值为1时，返回dir_empty属性和缩略图数据
            web: Option<i32>,
            ///是否只返回文件夹，0 返回所有，1 只返回文件夹，且属性只返回path字段
            folder: Option<i32>,
            ///是否返回dir_empty属性，0 不返回，1 返回
            #[serde(rename = "showempty")]
            show_empty: Option<i32>,
        }

        let params = Params {
            method: "list",
            dir: path,
            order: None,
            desc: None,
            start: None,
            limit: None,
            web: None,
            folder: None,
            show_empty: None,
        };
        self.request(Get, PATH, params, None::<()>)
    }
    async fn create_form(
        local_file: &str,
        progress_info: &ProgressInfo,
        progress_cb: Option<ProgressCallback>,
    ) -> Result<reqwest::multipart::Form, AppError> {
        let mut file = tokio::fs::File::open(local_file).await?;
        file.seek(SeekFrom::Start(progress_info.uploaded_bytes))
            .await?;

        let limited = file.take(progress_info.current_part_bytes);
        let reader_stream = ReaderStream::new(limited);

        let base_uploaded = progress_info.uploaded_bytes;
        let total_bytes = progress_info.total_bytes;
        let current_part = progress_info.current_part;
        let part_len = progress_info.current_part_bytes;

        let sent = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let sent_clone = sent.clone();
        let cb_opt = progress_cb.clone();

        // 将 reader\_stream 包装为会在读取时触发回调的流
        let stream = reader_stream.map_ok(move |chunk| {
            let len = chunk.len() as u64;
            let prev = sent_clone.fetch_add(len, std::sync::atomic::Ordering::Relaxed);
            if let Some(cb) = cb_opt.as_ref() {
                let mut cb_lock = cb.lock().unwrap();
                (cb_lock)(ProgressInfo {
                    total_bytes,
                    uploaded_bytes: base_uploaded.saturating_add(prev),
                    current_part,
                    current_part_bytes: len,
                });
            }
            chunk
        });

        let body = Body::wrap_stream(stream);
        let file_name = format!("file_{}", current_part);
        let part = reqwest::multipart::Part::stream_with_length(body, part_len)
            .file_name(file_name)
            .mime_str("application/octet-stream")?;

        Ok(reqwest::multipart::Form::new().part("file", part))
    }

    /// 上传文件（小文件） 需要注意的是有限制只能上传到 /apps/{app-name}/目录下，其他目录会返回 31064
    /// https://pan.baidu.com/union/doc/olkuuy5kz
    /// # Arguments
    /// * `local_file` - 本地文件路径(待上传文件的绝对路径)
    /// * `pcs_path` - 上传后使用的文件绝对路径，云盘的存储路径，需要注意的是有限制只能上传到 /apps/{app-name}/目录下，其他目录会返回 31064
    /// * `when_exists` - 上传的文件绝对路径冲突时的策略。0（默认：冲突时失败）1（冲突时覆盖） 2（冲突时重命名），其他值按照1 处理
    /// # Returns
    /// * `FileUpload` - 文件上传结果
    pub fn upload_single_file(
        &self,
        local_file: &str,
        pcs_path: &str,
        when_exists: i8,
    ) -> Result<PcsFileUploadResult, AppError> {
        let file = File::open(local_file)?;

        // 官网文档中为 /rest/2.0/pcs/file 主机名是 d.pcs.baidu.com
        // 如果用 pan.baidu.com/rest/2.0/xpan/file 会返回 413
        const PATH: &str = "/rest/2.0/pcs/file";
        // 正常小文件上传
        let mut path_buf = PathBuf::new();
        path_buf.push("/apps");
        path_buf.push(self.pcs_app.get_app_name());
        // 根据限制，只能上传到 /apps/{app-name}/目录下 因此需要检查并自动添加
        let path_src = PathBuf::from(pcs_path);
        let pcs_path: String = if path_src.starts_with(&path_buf) {
            path_src.as_path().to_string_lossy().to_string()
        } else {
            // 如果不是 /apps/{app-name}/ 目录下，自动添加
            path_buf.push(pcs_path.strip_prefix("/").unwrap());
            path_buf.as_path().to_string_lossy().to_string()
        };
        let pcs_path = pcs_path.as_str();

        let future = async {
            let form = Self::create_form(
                local_file,
                &ProgressInfo {
                    total_bytes: file.metadata().unwrap().len(),
                    uploaded_bytes: 0,
                    current_part: 0,
                    current_part_bytes: file.metadata().unwrap().len(),
                },
                None,
            )
            .await
            .unwrap();
            debug!("file len: {}", file.metadata().unwrap().len());
            self.client
                .post(format!("{}{}", PREFIX_FILE_SERVER, PATH))
                .query(&[
                    // 本接口固定为upload
                    ("method", "upload"),
                    ("access_token", self.access_token.as_str()),
                    // 上传的文件绝对路径
                    ("path", pcs_path),
                    // 上传的文件绝对路径冲突时的策略。fail（默认：冲突时失败）overwrite（冲突时覆盖） newcopy（冲突时重命名）
                    (
                        "ondup",
                        match when_exists {
                            0 => "fail",
                            1 => "overwrite",
                            2 => "newcopy",
                            _ => "overwrite",
                        },
                    ),
                ])
                .multipart(form)
                .send()
                .await
                .unwrap()
                .text()
                .await
        };
        // 文件上传使用单独的runtime
        let runtime = tokio::runtime::Runtime::new()?;
        let text = runtime.block_on(future)?;
        debug!("upload_single_file {} ->text: {}", pcs_path, text);
        let resp: serde_json::error::Result<PcsFileUploadResult> = serde_json::from_str(&text);
        match resp {
            Ok(v) => Ok(v),
            Err(_) => {
                let e: PcsApiError = serde_json::from_str(&text).unwrap_or(PcsApiError {
                    errno: i32::MIN,
                    err_msg: None,
                    request_id: None,
                    raw: text,
                });
                Err(e.into())
            }
        }
    }

    /// 分片上传文件（大文件）
    /// 这个接口不受“必须在 /apps/{app-name}/ 目录下”的限制
    /// https://pan.baidu.com/union/doc/3ksg0s9ye
    /// 由3个接口组成：
    /// 1. 预上传 file_slice_prepare
    /// 2. 分片上传 file_slice_upload
    /// 3. 创建文件（合并分片） file_slice_merge
    /// # Arguments
    /// * `local_file` - 本地文件路径(待上传文件的绝对路径)
    /// * `pcs_path` - 上传后使用的文件绝对路径，云盘的存储路径，需要注意的是有限制只能上传到 /apps/{app-name}/目录下，其他目录会返回 31064
    /// * `when_exists` - 上传的文件绝对路径冲突时的策略 1. 重命名， 3. 覆盖
    /// * `progress_callback` - 进度回调函数
    /// # Returns
    /// * `FileUpload` - 文件上传结果
    pub fn upload_large_file<F>(
        &self,
        local_file: &str,
        pcs_path: &str,
        police: PcsUploadPolicy,
        progress_callback: F,
    ) -> Result<PcsFileUploadResult, AppError>
    where
        F: FnMut(ProgressInfo) + Send + 'static,
    {
        info!("准备上传大文件 {}", local_file);

        let (task, fs_meta) = self.file_slice_prepare(local_file, pcs_path, &police)?;

        info!("预上传准备完成: {:?} , 文件信息 {:?}", task, fs_meta);

        let servers = self.get_upload_server(&task)?;
        let total_parts = task.block_list().len();
        let total_bytes = fs_meta.size;
        let mut uploaded_bytes: u64 = 0;

        let cb_arc: Arc<Mutex<dyn FnMut(ProgressInfo) + Send>> =
            Arc::new(Mutex::new(progress_callback));
        let slice_size = self.user_info.as_ref().unwrap().get_user_block_slice_size();

        let mut md5s: Vec<String> = Vec::with_capacity(total_parts);
        for i in 0..total_parts {
            let part_bytes = if i == total_parts - 1 {
                total_bytes - slice_size * (i as u64)
            } else {
                slice_size
            };
            let md5 = self.file_slice_upload(
                &fs_meta,
                &task,
                ProgressInfo {
                    total_bytes,
                    uploaded_bytes,
                    current_part: i as u32,
                    current_part_bytes: part_bytes,
                },
                &servers,
                Some(cb_arc.clone()),
            )?;
            info!("分片 {}/{} 上传完成 {}", i + 1, total_parts, md5);
            uploaded_bytes = uploaded_bytes.saturating_add(part_bytes);
            md5s.push(md5);
        }

        info!("所有分片上传完成: {:?}", md5s);
        self.file_slice_merge(task, fs_meta, md5s, &police)
    }

    /// 预上传文件
    /// # Arguments
    /// * `local_file` - 本地文件路径(待上传文件的绝对路径)
    /// * `pcs_path` - 上传后使用的文件绝对路径，云盘的存储路径，需要注意的是有限制只能上传到 /apps/{app-name}/目录下，其他目录会返回 31064
    /// * `police` - 上传的文件绝对路径冲突时的策略 注意仅支持 {重命名, 覆盖}, 否则默认为覆盖
    /// # Returns
    /// * `FileUploadSlice` - 文件上传结果
    /// * `FileSlice` - 文件分片信息
    /// # Errors
    /// * `AppError` - 文件上传失败
    pub(crate) fn file_slice_prepare(
        &self,
        local_file: &str,
        pcs_path: &str,
        police: &PcsUploadPolicy,
    ) -> Result<(PcsFileSlicePrepareResult, PcsFileSliceInfo), AppError> {
        const PATH: &str = "/rest/2.0/xpan/file";
        #[derive(Serialize)]
        struct Params<'a> {
            /// 本接口固定为`precreate`
            method: &'a str,
        }
        const PARAMS: Params = Params {
            method: "precreate",
        };
        #[derive(Serialize)]
        struct PreCreateAttributes<'a> {
            /// 上传后使用的文件绝对路径，需要urlencode
            path: &'a str,
            /// 文件和目录两种情况：上传文件时，表示文件的大小，单位B；上传目录时，表示目录的大小，目录的话大小默认为0
            size: u64,
            /// 是否为目录，0 文件，1 目录
            #[serde(rename = "isdir")]
            is_dir: i32,
            /// 文件各分片MD5数组的json串。block_list的含义如下，如果上传的文件小于4MB，其md5值（32位小写）即为block_list字符串数组的唯一元素；如果上传的文件大于4MB，需要将上传的文件按照4MB大小在本地切分成分片，不足4MB的分片自动成为最后一个分片，所有分片的md5值（32位小写）组成的字符串数组即为block_list。
            block_list: String,
            /// 固定值1
            #[serde(rename = "autoinit")]
            auto_init: i32,
            /// 文件命名策略。
            // 1 表示当path冲突时，进行重命名
            // 2 表示当path冲突且block_list不同时，进行重命名
            // 3 当云端存在同名文件时，对该文件进行覆盖
            #[serde(rename = "rtype")]
            r_type: Option<i32>,
            /// 上传ID
            #[serde(rename = "uploadid")]
            upload_id: Option<String>,
            /// 文件MD5，32位小写
            #[serde(alias = "content-md5")]
            content_md5: Option<String>,
            /// 文件校验段的MD5，32位小写，校验段对应文件前256KB
            #[serde(alias = "slice-md5")]
            slice_md5: Option<String>,
            /// 客户端创建时间(精确到秒)，默认为当前时间戳
            local_ctime: Option<i64>,
            /// 客户端修改时间(精确到秒)，默认为当前时间戳
            local_mtime: Option<i64>,
        }

        let fs_meta = get_file_block_list(&self.get_user_info()?, local_file)?;
        let payload = PreCreateAttributes {
            path: pcs_path,
            size: fs_meta.size,
            is_dir: 0,
            block_list: serde_json::to_string(fs_meta.block_list.as_slice())?,
            auto_init: 1,
            r_type: match police {
                PcsUploadPolicy::Rename => Some(1),
                PcsUploadPolicy::Overwrite => Some(3),
                PcsUploadPolicy::NewCopy => Some(2),
                _ => Some(3),
            },
            upload_id: None,
            content_md5: Some(fs_meta.content_md5.clone()),
            slice_md5: Some(fs_meta.slice_md5.clone()),
            local_ctime: Some(fs_meta.ctime),
            local_mtime: Some(fs_meta.mtime),
        };

        self.request(Post, PATH, PARAMS, Some(payload))
            .map(|x: PcsFileSlicePrepareResult| {
                if x.path.is_empty() {
                    PcsFileSlicePrepareResult {
                        path: pcs_path.to_string(),
                        upload_id: x.upload_id,
                        return_type: x.return_type,
                        block_list: x.block_list,
                    }
                } else {
                    x
                }
            })
            .map(|r| (r, fs_meta))
    }

    /// 分片上传文件
    /// 参见[官方文档](https://pan.baidu.com/union/doc/nksg0s9vi)
    pub(crate) fn file_slice_upload(
        &self,
        local_file: &PcsFileSliceInfo,
        upload_task: &PcsFileSlicePrepareResult,
        progress_info: ProgressInfo,
        server: &UploadServerResult,
        progress_cb: Option<ProgressCallback>,
    ) -> Result<String, AppError> {
        const PATH: &str = "/rest/2.0/pcs/superfile2";
        #[derive(Serialize, Deserialize, Debug)]
        struct UploadResultDTO {
            md5: String,
        }

        let upload_server = server
            .servers()
            .first()
            .or_else(|| server.bak_servers().first())
            .map(|s| s.server().clone())
            .unwrap_or_else(|| String::from(PREFIX_FILE_SERVER));
        info!("上传分片 {} 到服务器 {}", progress_info, upload_server);
        #[derive(Serialize)]
        struct Query<'a> {
            /// 本接口固定为 `upload`
            method: &'a str,
            /// 调用接口获取的access_token
            access_token: &'a str,
            /// 本接口固定为 `tmpfile`
            #[serde(rename = "type")]
            r#type: &'a str,
            /// 上传后使用的文件绝对路径，需要urlencode
            path: &'a str,
            /// 预上传precreate接口下发的uploadid
            #[serde(rename = "uploadid")]
            upload_id: &'a str,
            /// 分片序号，文件分片的位置序号，从0开始，参考上一个阶段预上传precreate接口返回的block_list
            #[serde(rename = "partseq")]
            part_seq: u32,
        }

        let fut = async {
            let form = Self::create_form(local_file.path.as_str(), &progress_info, progress_cb)
                .await
                .unwrap();
            self.client
                .post(format!("{}{}", upload_server, PATH))
                .query(&Query {
                    method: "upload",
                    access_token: self.access_token.as_str(),
                    r#type: "tmpfile",
                    path: upload_task.path().as_str(),
                    upload_id: upload_task.upload_id().as_str(),
                    part_seq: progress_info.current_part,
                })
                .multipart(form)
                .send()
                .await
                .unwrap()
                .text()
                .await
        };

        let runtime = tokio::runtime::Runtime::new()?;
        let text = runtime.block_on(fut)?;
        debug!("text: {}", text);
        let resp: serde_json::error::Result<UploadResultDTO> = serde_json::from_str(text.as_str());
        match resp {
            Ok(v) => Ok(v.md5),
            Err(_) => {
                let e: PcsApiError = serde_json::from_str(text.as_str()).unwrap_or(PcsApiError {
                    errno: i32::MIN,
                    err_msg: None,
                    request_id: None,
                    raw: text,
                });
                Err(e.into())
            }
        }
    }

    /// 创建文件
    /// 本接口将分片文件合并成一个文件 https://pan.baidu.com/union/doc/rksg0sa17
    pub fn file_slice_merge(
        &self,
        upload_task: PcsFileSlicePrepareResult,
        fs: PcsFileSliceInfo,
        hashes: Vec<String>,
        police: &PcsUploadPolicy,
    ) -> Result<PcsFileUploadResult, AppError> {
        const PATH: &str = "/rest/2.0/xpan/file";
        #[derive(Serialize)]
        struct Params<'a> {
            /// 本接口固定为`create`
            method: &'a str,
        }
        const PARAMS: Params = Params { method: "create" };
        #[derive(Serialize)]
        struct MergeAttributes<'a> {
            /// 上传后使用的文件绝对路径，需要urlencode
            path: &'a str,
            /// 文件或目录的大小，必须要和文件真实大小保持一致，需要与预上传precreate接口中的size保持一致
            size: u64,
            /// 是否目录，0 文件、1 目录，需要与预上传precreate接口中的isdir保持一致
            #[serde(rename = "isdir")]
            is_dir: &'a str,
            /// 文件各分片md5数组的json串
            // 需要与预上传precreate接口中的block_list保持一致，同时对应分片上传superfile2接口返回的md5，且要按照序号顺序排列，组成md5数组的json串。
            block_list: &'a str,
            /// 预上传precreate接口下发的uploadid
            #[serde(rename = "uploadid")]
            upload_id: &'a str,
            /// 文件命名策略，默认 `0`
            /// - `0` 为不重命名，返回冲突
            /// - `1` 为只要path冲突即重命名
            /// - `2` 为path冲突且block_list不同才重命名
            /// - `3` 为覆盖，需要与预上传precreate接口中的rtype保持一致
            #[serde(rename = "rtype")]
            r_type: Option<i32>,
            /// 客户端创建时间(精确到秒)，默认为当前时间戳
            local_ctime: Option<i64>,
            /// 客户端修改时间(精确到秒)，默认为当前时间戳
            local_mtime: Option<i64>,
            /// 图片压缩程度，有效值50、70、100（带此参数时，zip_sign 参数需要一并带上）
            zip_quality: Option<i32>,
            /// 未压缩原始图片文件真实md5（带此参数时，zip_quality 参数需要一并带上）
            zip_sign: Option<String>,
            /// 是否需要多版本支持, 默认为0 (带此参数会忽略重命名策略)
            /// - `1` 为支持
            /// - `0` 为不支持
            is_revision: Option<i32>,
            /// 上传方式
            /// - `1` 手动
            /// - `2` 批量上传
            /// - `3` 文件自动备份
            /// - `4` 相册自动备份
            /// - `5` 视频自动备份
            mode: Option<i32>,
            /// json字符串，orientation、width、height、recovery为必传字段，其他字段如果没有可以不传
            exif_info: Option<String>,
        }
        let block_list_json = serde_json::to_string(&hashes)?;
        self.request(
            Post,
            PATH,
            PARAMS,
            Some(MergeAttributes {
                path: upload_task.path().as_str(),
                size: fs.size,
                is_dir: "0",
                block_list: block_list_json.as_str(),
                upload_id: upload_task.upload_id.as_str(),
                r_type: match police {
                    PcsUploadPolicy::Rename => Some(1),
                    PcsUploadPolicy::Overwrite => Some(3),
                    PcsUploadPolicy::NewCopy => Some(2),
                    _ => Some(3),
                },
                local_ctime: Some(fs.ctime),
                local_mtime: Some(fs.mtime),
                zip_quality: None,
                zip_sign: None,
                is_revision: Some(1),
                mode: Some(2),
                exif_info: None,
            }),
        )
    }

    pub fn search_file(&self, name_or_path: &str) -> Result<PcsFileSearchResult, AppError> {
        const PATH: &str = "/rest/2.0/xpan/file";
        #[derive(Serialize)]
        struct Params<'a> {
            /// 本接口固定为`search`
            method: &'a str,
            /// 搜索关键字，最大30字符（UTF8格式）
            key: &'a str,
            /// 搜索目录，默认根目录
            dir: Option<&'a str>,
            /// 文件类型，1 视频、2 音频、3 图片、4 文档、5 应用、6 其他、7 种子
            category: Option<i32>,
            /// 分页
            page: Option<i32>,
            /// 默认为500，不能修改
            num: Option<i32>,
            /// 是否递归，带这个参数就会递归，否则不递归
            recursion: Option<i32>,
            /// 是否展示缩略图信息，带这个参数会返回缩略图信息，否则不展示缩略图信息
            web: Option<i32>,
            /// 设备ID，设备注册接口下发，硬件设备必传
            device_id: Option<&'a str>,
        }
        // 取 "/a/c/bddeeaaae.ext" 中的 "bddeeaaae" 的最后最多30字符
        let name = name_or_path
            .split("/")
            .last()
            .map(|s| {
                // s.to_string()
                //
                //     .chars()
                //     .rev()
                //     .take(30)
                //     .collect::<String>()
                //     .chars()
                //     .rev()
                //     .collect::<String>()
                s.rfind(".")
                    .map(|idx| &s[..idx])
                    .unwrap_or(s)
                    .chars()
                    .rev()
                    .take(30)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            })
            .unwrap_or_else(|| name_or_path.to_string());
        let path = if name_or_path.ends_with("/") {
            Some(name_or_path)
        } else {
            name_or_path.rfind("/").map(|idx| &name_or_path[..idx])
        };
        let params = Params {
            method: "search",
            key: name.as_str(),
            dir: path,
            category: None,
            page: None,
            num: None,
            recursion: Some(1),
            web: None,
            device_id: None,
        };
        self.request(Get, PATH, params, None::<()>)
    }

    /// 查询文件信息
    /// 参见[官方文档](https://pan.baidu.com/union/doc/Fksg0sbcm)
    /// 本接口可用于获取用户指定文件的meta信息。支持查询多个或一个文件的meta信息，meta信息包括文件名字、文件创建时间、文件的下载地址等。
    /// # Arguments
    /// * `down` - 是否需要下载地址，true 为是，false 为否
    /// * `fs_ids` - 文件id数组，数组中元素是uint64类型，数组大小上限是：100
    /// # Returns
    /// * `PcsFileMetaResult` - 文件信息结果
    /// # Errors
    /// * `AppError` - 文件信息查询失败
    pub fn get_file_info(
        &self,
        down: bool,
        fs_ids: Vec<u64>,
    ) -> Result<PcsFileMetaResult, AppError> {
        const PATH: &str = "/rest/2.0/xpan/multimedia";
        // 参数名称	类型	是否必填	示例	参数位置	描述
        // method	string	是	filemetas	URL参数	本接口固定为filemetas
        // access_token	string	是	12.a6b7dbd428f731035f771b8d15063f61.86400.1292922000-2346678-124328	URL参数	接口鉴权参数
        // fsids	array	是	[414244021542671,633507813519281]	URL参数	文件id数组，数组中元素是uint64类型，数组大小上限是：100
        // dlink	int	否	0	URL参数	是否需要下载地址，0为否，1为是，默认为0。获取到dlink后，参考下载文档进行下载操作
        // path	string	否	/123-571234	URL参数	查询共享目录或专属空间内文件时需要。含义并非待查询文件的路径，而是查询特定目录下文件的开关参数。
        // 共享目录格式： /uk-fsid
        // 其中uk为共享目录创建者id， fsid对应共享目录的fsid
        // 专属空间格式：/_pcs_.appdata/xpan/。此参数生效时，不会返回正常目录下文件的信息。
        // thumb	int	否	0	URL参数	是否需要缩略图地址，0为否，1为是，默认为0
        // extra	int	否	0	URL参数	图片是否需要拍摄时间、原图分辨率等其他信息，0 否、1 是，默认0
        // needmedia	int	否	0	URL参数	视频是否需要展示时长信息，needmedia=1时，返回 duration 信息时间单位为秒 （s），转换为向上取整。
        // 0 否、1 是，默认0
        // detail	int	否	0	URL参数	视频是否需要展示长，宽等信息。
        // 0 否、1 是，默认0。返回信息在media_info字段内，参考响应示例的视频文件。
        // device_id	string	否	144213733w02217w8v	URL参数	设备ID，硬件设备必传
        // from_apaas	int	否	1	URL参数	为下载地址(dlink)附加极速流量权益。用户通过此dlink产生下载行为时，消耗等同于文件大小的极速流量权益。此权益为付费权益，如需要购买极速流量权益服务，可联系商务合作邮箱：ext_mars-union@baidu.com 进行咨询，否则此参数无效。
        #[derive(Serialize)]
        struct Params<'a> {
            /// 本接口固定为`filemetas`
            method: &'a str,
            /// array 必须， 如 [414244021542671,633507813519281]	URL参数	文件id数组，数组中元素是uint64类型，数组大小上限是：100
            fsids: String,
            /// 是否返回下载链接，0 不返回，1 返回，默认为0
            // 注意：返回的下载链接有效期为8小时，过期后需要重新
            dlink: Option<i32>,
            /// 是否查询特定目录
            path: Option<&'a str>,
            /// 是否返回缩略图，0 不返回，1 返回，默认为0
            thumb: Option<i32>,
            /// 图片是否需要拍摄时间、原图分辨率等其他信息，0 否、1 是，默认0
            extra: Option<i32>,
            /// 视频是否需要展示时长信息，needmedia=1时，返回 duration 信息时间单位为秒 （s），转换为向上取整。0 否、1 是，默认0
            needmedia: Option<i32>,
            /// 视频是否需要展示长，宽等信息。0 否、1 是，默认0。返回信息在media_info字段内，参考响应示例的视频文件。
            detail: Option<i32>,
            /// 设备ID，硬件设备必传
            device_id: Option<&'a str>,
            /// 为下载地址(dlink)附加极速流量权益。用户通过此dlink产生下载行为时，消耗等同于文件大小的极速流量权益。
            from_apaas: Option<i32>,
        }
        let params = Params {
            method: "filemetas",
            fsids: serde_json::to_string(&fs_ids)?,
            dlink: down.then_some(1),
            path: None,
            thumb: None,
            extra: None,
            needmedia: None,
            detail: None,
            device_id: None,
            from_apaas: None,
        };
        self.request(Get, PATH, params, None::<()>)
    }

    /// 下载文件
    /// 参见[官方文档](https://pan.baidu.com/union/doc/pkuo3snyp)
    /// 本接口用于将用户存储在网盘的云端文件下载到本地。文件下载分为三个阶段：获取文件列表、查询文件信息、下载文件。第二个阶段查询文件信息依赖第一个阶段获取文件列表的结果，第三个阶段下载文件依赖第二阶段查询文件信息的结果，串行完成这三个阶段任务后，云端文件成功下载到本地。
    pub fn download<F>(
        &self,
        download_link: &str,
        local_path: &str,
        progress: Option<F>,
    ) -> Result<(), AppError>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        let full_url = format!(
            "{}&access_token={}",
            download_link,
            self.access_token.as_str()
        );
        let fut = async {
            let mut resp = self
                .client
                .get(full_url.as_str())
                .send()
                .await
                .map_err(|e| AppError::new(AppErrorType::Network, e.to_string().as_str(), None))?;

            let total_bytes = resp.content_length().unwrap_or(0);
            let mut file = tokio::fs::File::options()
                .create(true)
                .truncate(true)
                .write(true)
                .open(local_path)
                .await?;

            let mut downloaded: u64 = 0;
            while let Some(chunk) = resp
                .chunk()
                .await
                .map_err(|e| AppError::new(AppErrorType::Network, e.to_string().as_str(), None))?
            {
                file.write_all(&chunk).await?;
                downloaded += chunk.len() as u64;
                if let Some(ref cb) = progress {
                    cb(downloaded, total_bytes);
                }
            }
            file.flush().await?;
            Ok::<(), AppError>(())
        };
        self.runtime
            .block_on(fut)
            .map_err(|e| AppError::new(AppErrorType::Network, e.to_string().as_str(), None))
    }

    /// 通过文件路径反向查询百度网盘云端的文件ID
    /// # Arguments
    /// * `path` - 文件路径
    /// # Returns
    /// * `u64` - 文件ID
    /// # Errors
    /// * `AppError` - 文件ID查询失败
    pub(crate) fn get_fs_id_by_path(&self, path: &str) -> Result<u64, AppError> {
        // 百度网盘在下载等操作时 需要用fsid，但是一般我们都是通过path管理 需要维护一个表
        if path.ends_with("/") {
            // 目录
            return Err(AppError::new(
                AppErrorType::Unknown,
                "目录不支持获取fsid",
                None,
            ));
        }
        let binding = PathBuf::from(path.to_string());
        let parent = binding.parent().unwrap();
        // load cached path list
        let list = self.list_dir(parent.to_str().unwrap())?;
        for item in list.list {
            if item.path == path {
                return Ok(item.fs_id);
            }
        }
        Err(AppError::new(
            AppErrorType::Unknown,
            format!("未找到文件 {}", path).as_str(),
            None,
        ))
    }

    pub fn down_file<F>(
        &self,
        remote: &str,
        local_path: &str,
        progress: Option<F>,
    ) -> Result<(), AppError>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        self.get_fs_id_by_path(remote)
            .and_then(|fs_id| self.down_file_by_id(fs_id, local_path, progress))
    }

    pub fn down_file_by_id<F>(
        &self,
        fs_id: u64,
        local_path: &str,
        progress: Option<F>,
    ) -> Result<(), AppError>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        self.get_file_info(true, vec![fs_id]).and_then(|meta_res| {
            if meta_res.list.is_empty() {
                Err(AppError::new(
                    AppErrorType::Unknown,
                    format!("未找到文件 {}", fs_id).as_str(),
                    None,
                ))
            } else if meta_res.list[0].dlink.is_none() {
                Err(AppError::new(
                    AppErrorType::Unknown,
                    format!("未找到文件下载链接 {}", fs_id).as_str(),
                    None,
                ))
            } else {
                info!("准备下载文件 {:?}", meta_res.list[0]);
                let down_link = meta_res.list[0].dlink.as_ref().unwrap();
                self.download(down_link, local_path, progress)
            }
        })
    }

    /// 自定义功能： 备份指定文件到应用目录下
    /// 机制说明： 1. 如果文件小于 `FILE_MAX_SIZE` ，使用小文件上传接口，否则使用大文件上传接口
    /// # Arguments
    /// * `local_file` - 本地文件路径(待上传文件的绝对路径)
    /// * `pcs_path` - 上传后使用的文件绝对路径，云盘的存储路径，需要注意的是有限制只能上传到 /apps/{app-name}/目录下，其他目录会返回 31064
    /// # Returns
    /// * `FileUpload` - 文件上传结果
    /// # Errors
    /// * `AppError` - 文件上传失败
    pub fn backup_file(
        &mut self,
        local_file: &str,
        pcs_path: &str,
    ) -> Result<Vec<PcsFileUploadResult>, AppError> {
        let file = File::open(local_file)?;
        let mut rs: Vec<PcsFileUploadResult> = Vec::new();
        if file.metadata()?.is_file() {
            rs.push(self.upload_large_file(
                local_file,
                pcs_path,
                PcsUploadPolicy::Overwrite,
                |_| {},
            )?)
        } else if file.metadata()?.is_dir() {
            let prefix = PathBuf::from(pcs_path);
            for entry in std::fs::read_dir(local_file)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    let mut this_file = prefix.clone();
                    this_file.push(entry.path().strip_prefix(local_file).unwrap());
                    rs.push(self.upload_large_file(
                        entry.path().to_str().unwrap(),
                        this_file.as_path().to_str().unwrap(),
                        PcsUploadPolicy::Overwrite,
                        |_| {},
                    )?)
                }
            }
        }
        Ok(rs)
    }
}

/// 进度回调类型别名
pub type ProgressCallback = Arc<Mutex<dyn FnMut(ProgressInfo) + Send>>;

#[cfg(test)]
mod test {
    use crate::baidu_pcs_sdk::pcs::PcsUploadPolicy::Overwrite;
    use crate::baidu_pcs_sdk::pcs::{get_file_block_list, BaiduPcsClient, ProgressInfo};
    use crate::baidu_pcs_sdk::{BaiduPcsApp, PcsFileSlicePrepareResult};
    use std::env;
    const BAIDU_PCS_APP: BaiduPcsApp = BaiduPcsApp {
        app_name: env!("BAIDU_PCS_APP_NAME"),
        app_key: env!("BAIDU_PCS_APP_KEY"),
        app_secret: env!("BAIDU_PCS_APP_SECRET"),
    };

    #[test]
    fn test_get_user_info() {
        let client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let user_info = client.get_user_info().unwrap();
        println!("user_info: {:?}", user_info);
    }

    #[test]
    fn test_get_user_quota() {
        let client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let user_quota = client.get_user_quota(true, true).unwrap();
        println!("user_quota: {:?}", user_quota);
    }

    #[test]
    fn test_create_folder() {
        let client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let result = client.create_folder("/data/baidu-pcs-rs/backup").unwrap();
        println!("result: {:?}", result);
    }

    #[test]
    fn test_delete() {
        let client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let result = client
            .delete(
                &vec![
                    "/apps/stock-trunk/testsa".to_string(),
                    "/apps/stock-trunk/testsa2".to_string(),
                ],
                Some(true),
            )
            .unwrap();
        println!("result: {:?}", result);
    }

    #[test]
    fn test_list_dir() {
        let client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let result = client.list_dir("/我的资源");
        if result.is_err() {
            println!("error: {:?}", result.err().unwrap());
            assert!(false);
        } else {
            println!("result: {:?}", result.unwrap().list);
        }
    }

    #[test]
    fn test_upload_single_file() {
        let client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let result = client.upload_single_file("test/uploadtestdata/a.txt", "/backup/text.txt", 1);
        if result.is_err() {
            println!("error: {:?}", result.err().unwrap());
            assert!(false);
        } else {
            println!("result: {:?}", result.unwrap());
        }
    }

    #[test]
    fn test_prepare_file_upload() {
        let client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let result = client.file_slice_prepare(
            format!("{}/back.tar.gz", env::var("HOME").unwrap()).as_str(),
            "/apps/stock-trunk/backup/text.rar",
            &Overwrite,
        );
        if result.is_err() {
            println!("error: {:?}", result.err().unwrap());
            assert!(false);
        } else {
            println!("result: {:?}", result.unwrap());
        }
    }

    #[test]
    fn test_upload_file_slice() {
        let mut client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let task_file_meta = get_file_block_list(
            &client.get_user_info().unwrap(),
            format!("{}/back.tar.gz", env::var("HOME").unwrap()).as_str(),
        )
        .unwrap();
        let upload_task = PcsFileSlicePrepareResult {
            path: "/backup/text.rar".to_string(),
            upload_id: "P1-MTAuNDEuMTUuMTU6MTcwNjcyMjczNjo4NzkxMDIxNjg3NzQ1ODUyNTY5".to_string(),
            return_type: 1,
            block_list: vec![0, 1, 2],
        };
        let s = client
            .get_upload_server(&upload_task)
            .expect("获取上传服务器失败");
        let result = client.file_slice_upload(
            &task_file_meta,
            &upload_task,
            ProgressInfo {
                total_bytes: task_file_meta.size,
                uploaded_bytes: 0,
                current_part: 1,
                current_part_bytes: client.get_user_info().unwrap().get_user_block_slice_size(),
            },
            &s,
            None,
        );
        if result.is_err() {
            println!("error: {:?}", result.err().unwrap());
            assert!(false);
        } else {
            println!("result: {:?}", result.unwrap());
        }
    }

    #[test]
    fn test_backup_file() {
        let mut client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let result = client.backup_file("test/uploadtestdata/a.txt", "test/uploadtestdata/a.txt");
        if result.is_err() {
            println!("error: {:?}", result.err().unwrap());
            assert!(false);
        } else {
            println!("result: {:?}", result.unwrap());
        }
    }

    #[test]
    fn test_get_file_block_list() {
        let client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let result = get_file_block_list(
            &client.get_user_info().unwrap(),
            "test/uploadtestdata/a.txt",
        );
        if result.is_err() {
            println!("error: {:?}", result.err().unwrap());
            assert!(false);
        } else {
            let pcs_file_slice_info = result.unwrap();
            println!("result: {:?}", pcs_file_slice_info);
            assert_eq!(
                "d05f84cf5340d1ef0c5f6d6eb8ce13b8",
                pcs_file_slice_info.content_md5.as_str()
            );
            assert_eq!(271, pcs_file_slice_info.size);
        }
    }

    #[test]
    fn test_upload_large_file() {
        let mut pcs_client = BaiduPcsClient::new(
            "126.0a86437862dffb06d5d8773322fcb3d9.YCAJdSL-cWFVMa31pQgKFG9h5kDg8QV4nMnd7mT.t5qH1Q",
            BAIDU_PCS_APP,
        );
        let mut last_t = std::time::Instant::now();
        let mut last_uploaded = 0u64;

        let result = pcs_client.upload_large_file(
            "test/uploadtestdata/a.txt",
            "/backup/a.txt",
            Overwrite,
            |_| {},
        );
        if result.is_err() {
            println!("error: {:?}", result.err().unwrap());
            assert!(false);
        } else {
            println!("result: {:?}", result.unwrap());
        }
    }
}
