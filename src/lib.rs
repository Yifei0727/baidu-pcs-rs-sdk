pub mod baidu_pcs_sdk {
    use getset::Getters;
    use serde::{Deserialize, Deserializer, Serialize};
    use serde_json::Value;
    use std::error::Error;

    pub mod pcs;

    #[path = "pcs_device_auth_impl.rs"]
    pub mod pcs_device_auth;
    pub mod pcs_error;

    /// 百度网盘开放平台-我的应用
    /// [官方申请地址](https://pan.baidu.com/union/console/applist)
    #[derive(Debug)]
    pub struct BaiduPcsApp {
        /// 密钥信息-AppKey
        pub app_key: &'static str,
        /// 密钥信息-SecretKey
        pub app_secret: &'static str,
        /// 基本信息-应用名称
        pub app_name: &'static str,
    }

    /// 认证授权时调用的接口，错误时返回此类型
    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsError {
        error: String,
        error_description: String,
    }

    /// 网盘接口调用时，返回此类型
    /// 错误码 https://pan.baidu.com/union/doc/okumlx17r
    #[derive(Serialize, Deserialize, Debug)]
    pub struct PcsApiError {
        /// 表示具体错误码。 0 表示成功
        // 返回的json为 number， rust反序列化时，会报错，所以改为 i32
        #[serde(alias = "error_code")]
        errno: i32,
        /// 有关该错误的描述。
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(alias = "error_msg")]
        #[serde(alias = "errmsg")]
        err_msg: Option<String>,
        /// * `request_id`    String    发起请求的请求 Id。
        // 实际有的接口返回是的 number，有的是 string
        #[serde(deserialize_with = "from_str_or_int", default)]
        request_id: Option<String>,
        #[serde(skip)]
        pub(crate) raw: String,
    }

    #[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
    pub enum AppErrorType {
        /// 未知错误
        Unknown,
        /// 网络错误
        Network,
        /// 服务端错误
        Server,
        /// 客户端错误
        Client,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct AppError {
        pub error_type: AppErrorType,
        pub message: String,
        pub errno: Option<i64>,
    }

    #[derive(Debug, Deserialize, Getters)]
    #[getset(get = "pub with_prefix")]
    pub struct PcsAccessToken {
        /// 获取到的Access Token，Access Token是调用网盘开放API访问用户授权资源的凭证。
        access_token: String,
        /// Access Token的有效期，单位为秒。
        expires_in: u32,
        /// 用于刷新Access Token, 有效期为10年。
        refresh_token: String,
        /// Access Token 最终的访问权限，即用户的实际授权列表。
        scope: String,
        // 未定义的参数（实际有，忽略）
        #[serde(skip_serializing_if = "Option::is_none")]
        session_secret: Option<String>,
        // 未定义的参数（实际有，忽略）
        #[serde(skip_serializing_if = "Option::is_none")]
        session_key: Option<String>,
        /// 自行定义的参数，用于判断是否过期（对象创建时间）
        #[serde(skip)]
        born_at: i64,
    }

    #[derive(Serialize, Deserialize, Getters, Debug, Clone)]
    #[getset(get = "pub")]
    pub struct PcsUserInfo {
        /// `baidu_name`    string    百度账号
        baidu_name: String,
        /// `netdisk_name`    string    网盘账号
        netdisk_name: String,
        /// `avatar_url`    string    头像地址
        avatar_url: String,
        /// `vip_type`    int    会员类型，0普通用户、1普通会员、2超级会员
        vip_type: i32,
        /// `uk`    int    用户ID
        uk: u64,
    }

    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsDiskQuota {
        /// `total`    int    总空间大小，单位B
        total: u64,
        /// `expire`    bool    7天内是否有容量到期
        expire: bool,
        /// `used`    int    已使用大小，单位B
        used: u64,
        /// `free`    int    免费容量，单位B
        free: u64,
    }

    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsCreateFolderResult {
        /// `fs_id`    uint64    文件在云端的唯一标识ID
        fs_id: u64,
        /// `category`    int    分类类型, 6 文件夹
        category: i32,
        /// `path`    string    上传后使用的文件绝对路径
        path: String,
        /// `ctime`    int64    文件创建时间
        ctime: i64,
        /// `mtime`    int64    文件修改时间
        mtime: i64,
        /// `isdir`    int    是否目录，0 文件、1 目录
        #[serde(alias = "isdir")]
        is_dir: i32,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct PcsFileTask {
        errno: i32,
        path: String,
        task_id: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct PcsFileTaskOperationResult {
        info: Vec<PcsFileTask>,
        task_id: Option<String>,
    }

    #[derive(Serialize, Deserialize, Debug, Getters, Clone)]
    #[getset(get = "pub")]
    pub struct PcsFileItem {
        /// `fs_id` uint64 文件在云端的唯一标识ID
        fs_id: u64,
        /// `path` string 文件的绝对路径
        path: String,
        /// `server_filename` string 文件名称
        server_filename: String,
        /// `size` uint 文件大小，单位B
        size: u64,
        /// `server_mtime` int 文件在服务器修改时间
        server_mtime: i64,
        /// `server_ctime` int 文件在服务器创建时间
        server_ctime: i64,
        /// `local_mtime` int 文件在客户端修改时间
        local_mtime: i64,
        /// `local_ctime` int 文件在客户端创建时间
        local_ctime: i64,
        /// `isdir` uint 是否为目录，0 文件、1 目录
        #[serde(alias = "isdir")]
        is_dir: i32,
        /// `category` uint 文件类型，1 视频、2 音频、3 图片、4 文档、5 应用、6 其他、7 种子
        category: i32,
        /// `md5` string 云端哈希（非文件真实MD5），只有是文件类型时，该字段才存在
        md5: Option<String>,
        /// `dir_empty` int 该目录是否存在子目录，0为存在，1为不存在
        dir_empty: Option<i32>,
        /// `thumbs` array 包含三个尺寸的缩略图URL，仅当只有请求参数web=1且该条目分类为图片时存在
        thumbs: Option<Vec<String>>,
    }

    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsFileListResult {
        list: Vec<PcsFileItem>,
        guid: i64,
    }

    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsFileUploadResult {
        /// `path`    string    文件的绝对路径
        path: String,
        /// `size`    uint64    文件大小，单位B
        size: u64,
        /// `ctime`    int64    文件创建时间
        ctime: i64,
        /// `mtime`    int64    文件修改时间
        mtime: i64,
        /// `md5`    string    文件的MD5，只有提交文件时才返回，提交目录时没有该值
        md5: Option<String>,
        /// `fs_id`    uint64    文件在云端的唯一标识ID
        fs_id: u64,
    }

    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct UploadServerResult {
        client_ip: String,
        host: String,
        #[serde(deserialize_with = "from_str_or_int", default)]
        request_id: Option<String>,
        server_time: i64,
        #[serde(deserialize_with = "from_str_or_int", default)]
        sl: Option<String>,
        servers: Vec<Server>,
        bak_servers: Vec<Server>,
    }

    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub(crate) struct Server {
        server: String,
    }

    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsFileSlicePrepareResult {
        /// `path`    string    文件的绝对路径
        // 有时候返回没有这个字段
        #[serde(default)]
        path: String,
        /// `uploadid`    string    上传唯一ID标识此上传任务
        #[serde(alias = "uploadid")]
        upload_id: String,
        /// `return_type`    int    返回类型，系统内部状态字段
        return_type: i32,
        /// `block_list`    string    需要上传的分片序号列表，索引从0开始
        block_list: Vec<i32>,
    }
    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsFileMetaInfo {
        /// category 文件类型，1 视频、2 音频、3 图片、4 文档、5 应用、6 其他、7 种子
        category: i32,
        /// dlink 文件下载地址，参考下载文档进行下载操作。注意unicode解码处理。
        dlink: Option<String>,
        /// filename 文件名
        filename: String,
        /// isdir 是否是目录，为1表示目录，为0表示非目录
        #[serde(alias = "isdir", rename = "isdir")]
        is_dir: i32,
        /// server_ctime 文件的服务器创建Unix时间戳，单位秒
        server_ctime: i64,
        /// server_mtime 文件的服务器修改Unix时间戳，单位秒
        server_mtime: i64,
        /// size 文件大小，单位字节
        size: u64,
    }
    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsFileMetaResult {
        list: Vec<PcsFileMetaInfo>,
    }

    //has_more	int	是否还有下一页
    // list	array	文件列表
    // list[0] ["category"]	int	文件类型
    // list[0] ["fs_id"]	int	文件在云端的唯一标识
    // list[0] ["isdir"]	int	是否是目录，0为否，1为是
    // list[0] ["local_ctime"]	int	文件在客户端创建时间
    // list[0] ["local_mtime"]	int	文件在客户端修改时间
    // list[0] ["server_ctime"]	int	文件在服务端创建时间
    // list[0] ["server_mtime"]	int	文件在服务端修改时间
    // list[0] ["md5"]	string	云端哈希（非文件真实MD5）
    // list[0] ["size"]	int	文件大小
    // list[0] ["thumbs"]	string	缩略图地址
    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsFileSearchInfo {
        category: i32,
        fs_id: u64,
        is_dir: i32,
        local_ctime: i64,
        local_mtime: i64,
        server_ctime: i64,
        server_mtime: i64,
        md5: Option<String>,
        size: u64,
        thumbs: Option<Vec<String>>,
    }
    #[derive(Serialize, Deserialize, Debug, Getters)]
    #[getset(get = "pub")]
    pub struct PcsFileSearchResult {
        has_more: i32,
        list: Vec<PcsFileSearchInfo>,
    }

    impl BaiduPcsApp {
        pub fn get_app_key(&self) -> String {
            self.app_key.to_string()
        }
        pub fn get_app_secret(&self) -> String {
            self.app_secret.to_string()
        }
        pub fn get_app_name(&self) -> String {
            self.app_name.to_string()
        }
    }
    impl Error for AppError {}

    impl PcsAccessToken {
        pub fn new(access_token: &str, expires_in: u32, refresh_token: &str, scope: &str) -> Self {
            Self {
                access_token: access_token.to_string(),
                expires_in,
                refresh_token: refresh_token.to_string(),
                scope: scope.to_string(),
                session_key: None,
                session_secret: None,
                born_at: chrono::Utc::now().timestamp(),
            }
        }
        pub fn is_expired(&self) -> bool {
            (chrono::Utc::now().timestamp() + 600) > (self.born_at + self.expires_in as i64)
        }

        pub fn is_need_refresh(&self) -> bool {
            // 一般有效期是30天， 小于 7 天 则刷新
            (chrono::Utc::now().timestamp() + 7 * 24 * 3600)
                < (self.born_at + self.expires_in as i64)
        }
    }

    /// 反序列化时，支持 string 和 number 或者空，避免服务器返回的数据不规范导致反序列化失败
    fn from_str_or_int<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer);
        if value.is_err() {
            // 无法解析，返回 None
            return Ok(None);
        }
        match value? {
            Value::String(s) => Ok(Some(s)),
            Value::Number(n) => Ok(Some(n.to_string())),
            _ => Ok(None),
        }
    }

    impl PcsUserInfo {
        /// 返回当前用户账号允许上次文件的分片大小，用于上传大文件时的文件切片
        // https://pan.baidu.com/union/doc/nksg0s9vi
        // 如果文件大小小于等于4MB，无需切片，直接上传即可
        // 授权用户为普通用户时，单个分片大小固定为4MB，单文件总大小上限为4GB
        // 授权用户为普通会员时，单个分片大小上限为16MB，单文件总大小上限为10GB
        // 授权用户为超级会员时，用户单个分片大小上限为32MB，单文件总大小上限为20GB
        pub fn get_user_block_slice_size(&self) -> u64 {
            // 授权用户为普通用户时，单个分片大小固定为4MB，单文件总大小上限为4GB
            // 授权用户为普通会员时，单个分片大小上限为16MB，单文件总大小上限为10GB
            // 授权用户为超级会员时，用户单个分片大小上限为32MB，单文件总大小上限为20GB
            // 目前只支持 4MB
            match self.vip_type {
                0 => {
                    // 普通用户
                    4 * 1024 * 1024
                }
                1 => {
                    // 普通会员
                    16 * 1024 * 1024
                }
                2 => {
                    // 超级会员
                    32 * 1024 * 1024
                }
                _ => {
                    // 其他类型
                    4 * 1024 * 1024
                }
            }
        }

        /// 返回用户最大可上传文件大小
        // https://pan.baidu.com/union/doc/nksg0s9vi
        // 如果文件大小小于等于4MB，无需切片，直接上传即可
        // 授权用户为普通用户时，单个分片大小固定为4MB，单文件总大小上限为4GB
        // 授权用户为普通会员时，单个分片大小上限为16MB，单文件总大小上限为10GB
        // 授权用户为超级会员时，用户单个分片大小上限为32MB，单文件总大小上限为20GB
        pub fn get_user_max_upload_file_size(&self) -> u64 {
            match self.vip_type {
                0 => {
                    // 普通用户
                    4 * 1024 * 1024 * 1024
                }
                1 => {
                    // 普通会员
                    10 * 1024 * 1024 * 1024
                }
                2 => {
                    // 超级会员
                    20 * 1024 * 1024 * 1024
                }
                _ => {
                    // 其他类型
                    4 * 1024 * 1024 * 1024
                }
            }
        }
    }
}

// 顶层模块：自定义 DNS 解析能力（源文件位于 src/dns.rs）
pub mod dns;
