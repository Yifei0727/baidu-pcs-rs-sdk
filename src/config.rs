use crate::auth::device_auth_with_dns;
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
    /// 自定义 DNS 服务器，逗号分隔，例如："8.8.8.8,1.1.1.1"（可为空，空则使用系统默认）
    pub dns: Option<String>,
    /// 备份任务配置（本地与远程路径）
    pub backup: Option<BackupConfig>,
}

/// 备份任务路径配置
#[derive(Deserialize, Serialize, Clone)]
pub struct BackupConfig {
    /// 本地备份源目录
    pub local_path: String,
    /// 远程备份目标目录
    pub remote_path: String,
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
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            log::error!("创建配置目录失败 {}: {}", parent.display(), e);
            return;
        }
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(config_file) = File::create(path.as_path()) {
            let _ = config_file.set_permissions(fs::Permissions::from_mode(0o600));
        }
    }
    match File::create(path.as_path()) {
        Ok(mut file) => match toml::to_string(&config) {
            Ok(config_str) => {
                if let Err(e) = file.write_all(config_str.as_bytes()) {
                    log::error!("写入配置文件失败 {}: {}", path.display(), e);
                }
            }
            Err(e) => {
                log::error!("序列化配置失败: {}", e);
            }
        },
        Err(e) => {
            log::error!("创建配置文件失败 {}: {}", path.display(), e);
        }
    }
}

pub fn config_load_or_init(
    custom_config: Option<&String>,
    local: Option<String>,
    remote: Option<String>,
    dns: Option<&str>,
) -> Config {
    use std::io::Read;
    let path = get_config_file_path(custom_config);
    // 如果配置文件不存在则创建
    if !path.exists() {
        info!(
            "配置文件 {} 不存在，正在创建默认配置文件并进行认证...",
            path.display()
        );
        let local_root = local.unwrap_or_else(|| "/data/backup/".to_string());
        let remote_root = remote.unwrap_or_else(|| "/".to_string());
        let pcs_token: PcsAccessToken = device_auth_with_dns(dns);
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
            dns: dns.map(|s| s.to_string()),
            backup: None,
        };
        save_or_update_config(&mut config, custom_config);
    }
    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("无法打开配置文件 {}: {}", path.display(), e);
            std::process::exit(1);
        }
    };
    let mut contents = String::new();
    if let Err(e) = file.read_to_string(&mut contents) {
        eprintln!("无法读取配置文件 {}: {}", path.display(), e);
        std::process::exit(1);
    }
    // 避免在日志中打印包含 access_token / refresh_token 的配置内容
    debug!("成功读取配置文件: {}", path.display());
    match toml::from_str::<Config>(&contents) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("配置文件格式无效 {}: {}", path.display(), e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::get_config_file_path;

    #[test]
    fn test_get_config_file_path() {
        let path = get_config_file_path(None);
        let expected = directories::BaseDirs::new()
            .unwrap()
            .config_dir()
            .join("baidu-pcs-rs")
            .join("config.toml");
        assert_eq!(path, expected);
    }
}
