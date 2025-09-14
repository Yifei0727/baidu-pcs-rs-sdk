use crate::auth::device_auth;
use baidu_pcs_rs_sdk::baidu_pcs_sdk::PcsAccessToken;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub baidu_pan: BaiduPan,
    pub local_pan: LocalConfig,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct BaiduPan {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub root_path: String,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct LocalConfig {
    pub root_path: String,
    pub include_prefix: Option<bool>,
}

impl Config {
    pub fn update_token(&mut self, ticket: PcsAccessToken) {
        self.baidu_pan.access_token = ticket.get_access_token().to_string();
        self.baidu_pan.refresh_token = ticket.get_refresh_token().to_string();
        self.baidu_pan.expires_at = ticket.get_born_at() + *ticket.get_expires_in() as i64;
    }
    pub fn is_need_refresh_token(&self) -> bool {
        self.baidu_pan.is_need_refresh_token()
    }
}

impl BaiduPan {
    /// 凭据是否需要刷新
    pub fn is_need_refresh_token(&self) -> bool {
        chrono::Utc::now().timestamp() + 7 * 24 * 3600 > self.expires_at
    }
}

pub fn get_config_file_path(custom_config: Option<&String>) -> PathBuf {
    let mut path = PathBuf::new();
    match custom_config {
        Some(c) if !c.trim().is_empty() => {
            path.push(c.trim());
        }
        _ => {
            if let Some(base_dir) = directories::BaseDirs::new() {
                path.push(base_dir.config_dir());
                path.push("baidu-pcs-rs");
                path.push("config.toml");
            }
        }
    }
    path
}

pub fn save_or_update_config(config: &mut Config, custom_config: Option<&String>) {
    use std::io::prelude::*;
    let path = get_config_file_path(custom_config);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let config_file = File::create(path.as_path()).unwrap();
        config_file
            .set_permissions(fs::Permissions::from_mode(0o600))
            .unwrap();
    }
    let mut file = File::create(path.as_path()).unwrap();
    let config_str = toml::to_string(&config).unwrap();
    file.write_all(config_str.as_bytes()).unwrap();
}

pub fn config_load_or_init(
    custom_config: Option<&String>,
    local: Option<String>,
    remote: Option<String>,
) -> Config {
    use std::io::Read;
    let path = get_config_file_path(custom_config);
    // 如果配置文件不存在则创建
    if !path.exists() {
        info!(
            "配置文件 {} 不存在，正在创建默认配置文件并进行认证...",
            path.to_str().unwrap()
        );
        let local_root = local.unwrap_or_else(|| "/data/backup/".to_string());
        let remote_root = remote.unwrap_or_else(|| "/".to_string());
        let pcs_token: PcsAccessToken = device_auth();
        let mut config: Config = Config {
            baidu_pan: BaiduPan {
                access_token: pcs_token.get_access_token().to_string(),
                refresh_token: pcs_token.get_refresh_token().to_string(),
                expires_at: *pcs_token.get_born_at(),
                root_path: remote_root.to_string(),
            },
            local_pan: LocalConfig {
                root_path: local_root.to_string(),
                include_prefix: Some(false),
            },
        };
        save_or_update_config(&mut config, custom_config);
    }
    let mut file = File::open(path.clone()).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    debug!("config => {}", contents);
    let config_a = toml::from_str::<Config>(&contents);
    config_a.expect("config file is not valid")
}

#[cfg(test)]
mod tests {
    use crate::config::get_config_file_path;
    use std::env;

    #[test]
    fn test_get_config_file_path() {
        let path = get_config_file_path(None);
        assert_eq!(
            path.to_str().unwrap(),
            format!(
                "{}/.config/baidu-pcs-rs/config.toml",
                env::var("HOME").unwrap()
            )
        );
    }
}
