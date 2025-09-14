use crate::baidu_pcs_sdk::AppErrorType::{Client, Network, Server};
use crate::baidu_pcs_sdk::{AppError, AppErrorType, PcsApiError, PcsError};
use std::fmt::{Display, Error};

/// https://pan.baidu.com/union/doc/okumlx17r
// 2/31023	参数错误	1.检查必选参数是否都已填写；
// 2.检查参数位置，有的参数是在url里，有的是在body里；
// 3.检查每个参数的值是否正确。
// 111	access token 失效	更新access token。
// -6	身份验证失败	1.access_token 是否有效;
// 2.授权是否成功；
// 3.参考接入授权FAQ；
// 4.阅读文档《使用入门->接入授权》章节。
// 6	不允许接入用户数据	建议10分钟之后用户再进行授权重试。
// 31034	命中接口频控	接口请求过于频繁，注意控制。
// 2131	该分享不存在	检查tag是否传的一个空文件夹。
// 10	转存文件已经存在	转存文件已经存在
// -3	文件不存在
// 文件不存在
// -31066	文件不存在	文件不存在
// 11	自己发送的分享	自己发送的分享
// 255	转存数量太多	转存数量太多
// 12	批量转存出错	参数错误，检查转存源和目的是不是同一个uid，正常不应该是一个 uid
// -1	权益已过期	权益已过期

pub(crate) fn translate_error_to_string(error: AppError) -> String {
    if error.error_type == Server {
        if let Some(errno) = error.errno {
            return try_translate_errno(&error.message, errno);
        }
    }
    error.message
}

impl AppError {
    pub fn new(error_type: AppErrorType, message: &str, errno: Option<i64>) -> Self {
        Self {
            error_type,
            message: message.to_string(),
            errno,
        }
    }
}

impl From<AppError> for String {
    fn from(value: AppError) -> Self {
        translate_error_to_string(value)
    }
}

impl From<Error> for AppError {
    fn from(e: Error) -> Self {
        AppError::new(Client, e.to_string().as_str(), None)
    }
}

impl From<PcsError> for AppError {
    fn from(e: PcsError) -> Self {
        AppError::new(
            Server,
            format!("{}:{}", e.error, e.error_description).as_str(),
            None,
        )
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.error_type {
            Client => write!(f, "Client Error: {}", self.message),
            Network => write!(f, "Network Error: {}", self.message),
            Server => try_translate_errno(&self.message, self.errno.unwrap_or(0)).fmt(f),
            _ => write!(f, "Unknown Error: {}", self.message),
        }
    }
}

fn try_translate_errno(msg: &String, errno: i64) -> String {
    if msg.trim().is_empty() {
        return match errno {
            2 => "参数错误".to_string(),
            6 => "不允许接入用户数据".to_string(),
            10 => "转存文件已经存在".to_string(),
            11 => "自己发送的分享".to_string(),
            12 => "批量转存出错".to_string(),
            111 => "access token 失效".to_string(),
            255 => "转存数量太多".to_string(),
            2131 => "该分享不存在".to_string(),
            31023 => "参数错误".to_string(),
            31024 => "没有申请上传权限".to_string(), //申请开通上传权限
            31034 => "命中接口频控".to_string(),
            31061 => "文件已存在".to_string(),
            31064 => "上传路径权限".to_string(), //path 上传文件的绝对路径格式：/apps/申请接入时填写的产品名称请参考《能力说明->限制条件->目录限制》
            31190 => "文件不存在".to_string(),
            31299 => "第一个分片的大小小于4MB".to_string(),
            31363 => "分片缺失".to_string(),
            31365 => "文件总大小超限".to_string(),
            -31066 => "文件不存在".to_string(),
            -1 => "权益已过期".to_string(),
            -3 => "文件不存在".to_string(),
            -6 => "身份验证失败".to_string(),
            -7 => "文件或目录无权访问".to_string(),
            -8 => "文件或目录已存在".to_string(),
            -9 => "文件或目录不存在".to_string(),
            -10 => "容量不足(云端容量已满)".to_string(),
            _ => msg.to_string(),
        };
    }
    msg.to_string()
}

impl From<PcsApiError> for AppError {
    fn from(e: PcsApiError) -> Self {
        if e.errno == i32::MIN {
            AppError::new(Server, e.raw.as_str(), None)
        } else {
            AppError::new(
                Server,
                e.err_msg.unwrap_or(e.raw).to_string().as_str(),
                Some(e.errno as i64),
            )
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::new(Client, e.to_string().as_str(), None)
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::new(Network, e.to_string().as_str(), None)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::new(Client, e.to_string().as_str(), None)
    }
}
