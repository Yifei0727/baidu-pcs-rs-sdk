use crate::config::{save_or_update_config, Config};
use crate::BAIDU_PCS_APP;
use baidu_pcs_rs_sdk::baidu_pcs_sdk::pcs_device_auth::{BaiduPanClient, BaiduPanDeviceAuthClient};
use baidu_pcs_rs_sdk::baidu_pcs_sdk::PcsAccessToken;
use log::{debug, error, info};
use std::thread::sleep;

pub fn device_auth() -> PcsAccessToken {
    device_auth_with_dns(None)
}

pub fn device_auth_with_dns(dns: Option<&str>) -> PcsAccessToken {
    debug!("device_auth");
    let client: BaiduPanClient = BaiduPanDeviceAuthClient::with_dns(BAIDU_PCS_APP, dns);
    let ticket = client.get_user_code();
    println!(
        "请在浏览器中打开网址: {} \n并输入验证码: {}",
        ticket.get_verification_url(),
        ticket.get_user_code()
    );
    loop {
        sleep(std::time::Duration::from_secs(
            ticket.get_interval().unsigned_abs() + 1,
        ));
        let access_token = client.get_access_token(ticket.get_device_code().clone());
        match access_token {
            Ok(token) => {
                info!("device auth success");
                return token;
            }
            Err(error) => {
                info!("error: {:?}  try again ...", error);
                match error.error().as_str() {
                    "pcs sdk error" => {
                        panic!("{}", error.error_description())
                    }
                    "authorization_pending" => {
                        continue;
                    }
                    _ => {
                        // "invalid_grant"
                        error!("{}", error.error());
                        return device_auth_with_dns(dns);
                    }
                }
            }
        }
    }
}

pub fn renew_token(config: &mut Config, custom_config: Option<&String>, dns: Option<&str>) {
    let auth_client: BaiduPanClient = BaiduPanDeviceAuthClient::with_dns(BAIDU_PCS_APP, dns);
    let token = auth_client.refresh_access_token(&PcsAccessToken::new(
        config.baidu_pan.access_token.as_str(),
        (config.baidu_pan.expires_at - chrono::Utc::now().timestamp()) as u32,
        config.baidu_pan.refresh_token.as_str(),
        "basic,netdisk",
    ));
    match token {
        Ok(token) => {
            config.update_token(token);
            save_or_update_config(config, custom_config);
        }
        Err(error) => {
            error!(
                "refresh token failed: {} {}",
                error.error(),
                error.error_description()
            );
            info!("尝试重新认证授权...");
            let pcs_token: PcsAccessToken = device_auth_with_dns(dns);
            config.update_token(pcs_token);
            save_or_update_config(config, custom_config);
        }
    }
}

pub fn first_app_use() -> bool {
    println!(
        "\
        **********************************************\n\
        欢迎使用本程序({})，在使用前请注意:\n\
        **需要您授权** 本程序 访问您的百度网盘(pan.baidu.com)账户数据\n\
        如果您不同意或者没有百度网盘账户使用则请直接关闭本程序\n\
        如果同意使用则请按提示操作:\n\
        **********************************************\n\
        Control C 退出  |  按回车键继续...\n\
        **********************************************
        ",
        BAIDU_PCS_APP.get_app_name()
    );
    // 如果 readline 成功则继续 否则 一般是 control c 退出
    std::io::stdin().read_line(&mut String::new()).is_ok()
}
