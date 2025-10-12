use crate::baidu_pcs_sdk::{BaiduPcsApp, PcsAccessToken, PcsError};
use getset::Getters;
use log::info;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// 百度网盘--授权认证客户端
pub struct BaiduPanClient {
    runtime: tokio::runtime::Runtime,
    client: reqwest::Client,
    pcs_node: BaiduPcsApp,
    /// 指定的 DNS 服务器（逗号分隔），用于网络请求解析域名
    dns: Option<String>,
}

#[derive(Debug, Deserialize, Getters)]
#[getset(get = "pub with_prefix")]
pub struct PcsDeviceTicket {
    /// 设备码
    device_code: String,
    /// 用户码
    user_code: String,
    /// 用户验证地址
    verification_url: String,
    /// 设备码的有效期，单位为秒
    expires_in: i64,
    /// 用户轮询时间间隔，单位为秒
    interval: i64,
    /// 用户验证二维码地址
    qrcode_url: String,
}

/// 设备码模式授权
/// https://pan.baidu.com/union/doc/fl1x114ti
pub trait BaiduPanDeviceAuthClient {
    /// 构建客户端（提供应用的 AppKey 和 SecretKey）
    /// https://pan.baidu.com/union/doc/fl1x114ti
    fn with(app: BaiduPcsApp) -> Self;
    /// 构建带 DNS 指定的客户端
    fn with_dns(app: BaiduPcsApp, dns: Option<&str>) -> Self;
    /// 1. 获取设备码、用户码
    //
    // 既获取设备码 device code 、用户码 user code，依赖于以下请求链接：
    //
    // https://openapi.baidu.com/oauth/2.0/device/code?response_type=device_code&
    // client_id=您应用的AppKey&
    // scope=basic,netdisk
    //
    // 关于应用的相关信息，您可在控制台，点进去您对应的应用，查看应用详情获得。
    /// # Returns
    /// * `DeviceTicket` - DeviceTicket一次性凭证
    fn get_user_code(&self) -> PcsDeviceTicket;
    /// 3. 用 Device Code 轮询换取 Access Token
    //
    // 通过 Device Code 轮询换取 Access Token。换取Access Token 的实现依赖于以下请求链接：
    //
    // https://openapi.baidu.com/oauth/2.0/token?grant_type=device_token&
    // code=第一步生成的设备码device_code&
    // client_id=您应用的AppKey&
    // client_secret=您应用的SecretKey
    //
    // 关于应用的相关信息，您可在控制台，点进去您对应的应用，查看应用详情获得。
    // 注意：轮询此接口每两次请求时间间隔应大于5秒。
    /// # Arguments
    /// * `device_code` - 设备码(从 get_user_code 返回的 `DeviceTicket` 中获取)
    /// # Returns
    /// * `PcsAccessToken` - PcsAccessToken
    fn get_access_token(&self, device_code: String) -> Result<PcsAccessToken, PcsError>;
    /// 刷新 Access Token
    // 设备码模式下支持刷新 Access Token。
    //
    // 通过 Refresh Token 刷新，具体依赖于以下链接：
    //
    // GET https://openapi.baidu.com/oauth/2.0/token?
    // grant_type=refresh_token&
    // refresh_token=Refresh Token的值&
    // client_id=您应用的AppKey&
    // client_secret=您应用的SecretKey
    //
    // 以上链接示例中参数仅给出了必选参数。
    // 关于应用的相关信息，您可在控制台，点进去您对应的应用，查看应用详情获得。
    // 关于 Refresh Token的值，在换取 Access Token 凭证时，您可在响应信息中拿到。
    /// # Arguments
    /// * `pcs_access_token` - 旧的 PcsAccessToken(包含 refresh_token)
    /// # Returns
    /// * `PcsAccessToken` - 新的 PcsAccessToken(包含 refresh_token)
    fn refresh_access_token(
        &self,
        pcs_access_token: &PcsAccessToken,
    ) -> Result<PcsAccessToken, PcsError>;

    /// 获取应用的 App 名称
    fn get_appname(&self) -> String;
}
impl BaiduPanClient {
    fn request<T, R>(&self, prefix: &str, params: T) -> Result<R, PcsError>
    where
        T: Serialize,
        R: DeserializeOwned,
    {
        let future = async {
            let text = self
                .client
                .get(prefix)
                .query(&params)
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();

            info!("request: response=> {}", text.as_str());
            let result: Result<R, _> = serde_json::from_str(text.as_str());
            // 如果尝试反序列化失败，一般说明调用接口失败，尝试反序列化错误信息
            match result {
                Ok(r) => Ok(r),
                Err(reason) => {
                    let result: Result<PcsError, _> = serde_json::from_str(text.as_str());
                    match result {
                        Ok(r) => Err(r),
                        Err(e) => Err(PcsError {
                            error: String::from("pcs sdk error"),
                            error_description: format!(
                                "反序列化错误信息失败: {:?} {:?} {:?}",
                                text, reason, e
                            ),
                        }),
                    }
                }
            }
        };
        self.runtime.block_on(future)
    }
}

impl BaiduPanDeviceAuthClient for BaiduPanClient {
    fn with(app: BaiduPcsApp) -> Self {
        Self::with_dns(app, None)
    }

    fn with_dns(app: BaiduPcsApp, dns: Option<&str>) -> Self {
        let mut builder = reqwest::Client::builder();
        // 应用自定义 DNS（预解析固定域名）
        builder = crate::dns::apply_custom_dns(builder, dns, &["openapi.baidu.com"]);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("User-Agent", "pan.baidu.com".parse().unwrap());
        Self {
            client: builder.default_headers(headers).build().unwrap(),
            pcs_node: app,
            runtime: tokio::runtime::Runtime::new().unwrap(),
            dns: dns.map(|s| s.to_string()),
        }
    }

    fn get_user_code(&self) -> PcsDeviceTicket {
        const URL: &str = "https://openapi.baidu.com/oauth/2.0/device/code";
        #[derive(Serialize)]
        struct Params<'a> {
            response_type: &'a str,
            client_id: String,
            scope: &'a str,
        }

        let params = Params {
            response_type: "device_code",
            client_id: self.pcs_node.get_app_key(),
            scope: "basic,netdisk",
        };
        self.request(URL, params).unwrap()
    }

    fn get_access_token(&self, device_code: String) -> Result<PcsAccessToken, PcsError> {
        const URL: &str = "https://openapi.baidu.com/oauth/2.0/token";
        #[derive(Serialize)]
        struct Params<'a> {
            grant_type: &'a str,
            code: String,
            client_id: String,
            client_secret: String,
        }

        let params = Params {
            grant_type: "device_token",
            code: device_code,
            client_id: self.pcs_node.get_app_key(),
            client_secret: self.pcs_node.get_app_secret(),
        };
        self.request(URL, params).map(|mut token: PcsAccessToken| {
            token.born_at = chrono::Utc::now().timestamp();
            token
        })
    }

    fn refresh_access_token(
        &self,
        pcs_access_token: &PcsAccessToken,
    ) -> Result<PcsAccessToken, PcsError> {
        const URL: &str = "https://openapi.baidu.com/oauth/2.0/token";
        #[derive(Serialize)]
        struct Params<'a> {
            grant_type: &'a str,
            refresh_token: String,
            client_id: String,
            client_secret: String,
        }

        let params = Params {
            grant_type: "refresh_token",
            refresh_token: String::from(pcs_access_token.get_refresh_token()),
            client_id: self.pcs_node.get_app_key(),
            client_secret: self.pcs_node.get_app_secret(),
        };
        self.request(URL, params).map(|mut token: PcsAccessToken| {
            token.born_at = chrono::Utc::now().timestamp();
            token
        })
    }
    fn get_appname(&self) -> String {
        self.pcs_node.get_app_name()
    }
}

#[cfg(test)]
mod test {
    use crate::baidu_pcs_sdk::pcs_device_auth::{
        BaiduPanClient, BaiduPanDeviceAuthClient, PcsDeviceTicket,
    };
    use crate::baidu_pcs_sdk::{BaiduPcsApp, PcsAccessToken};
    use std::env;

    const BAIDU_PCS_APP: BaiduPcsApp = BaiduPcsApp {
        app_key: env!("BAIDU_PCS_APP_KEY"),
        app_secret: env!("BAIDU_PCS_APP_SECRET"),
        app_name: env!("BAIDU_PCS_APP_NAME"),
    };
    #[test]
    fn test_get_user_code() {
        log::log_enabled!(log::Level::Debug);

        let client: BaiduPanClient = BaiduPanDeviceAuthClient::with(BAIDU_PCS_APP);
        let user_code: PcsDeviceTicket = client.get_user_code();

        println!("user_code: {:?}", user_code);
    }

    #[test]
    fn test_get_access_token() {
        log::log_enabled!(log::Level::Debug);
        let client: BaiduPanClient = BaiduPanDeviceAuthClient::with(BAIDU_PCS_APP);
        let access_token: PcsAccessToken = client
            .get_access_token(String::from("eb5ce9ded31f6a3778ab3f66ec330820"))
            .unwrap();
        println!("access_token: {:?}", access_token);
    }

    #[test]
    fn test_refresh_access_token() {
        log::log_enabled!(log::Level::Debug);

        let client: BaiduPanClient = BaiduPanDeviceAuthClient::with(BAIDU_PCS_APP);
        let access_token: PcsAccessToken = PcsAccessToken::new(
            "126.e894e87c7f7771a4bcae5cf27955b389.YB_Z3FqgglDb1qeIUif--0gZksBUPzhagunVKoQ.Mj7EyA",
            2592000,
            "127.ac9946bbdc8e02749e1e4f6b2fb647c5.Y7-MgtiWddKPGnF4UQnDmMh01SNfu5GGfvMK82Y.guj_XA",
            "basic netdisk",
        );

        println!("access_token: {:?}", access_token);
        let access_token: PcsAccessToken = client.refresh_access_token(&access_token).unwrap();
        println!("access_token: {:?}", access_token);
    }
}
