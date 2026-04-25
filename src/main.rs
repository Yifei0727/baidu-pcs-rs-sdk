// 扫描输入的文件/文件夹 并于百度网盘中的文件/文件夹进行比对
// 若不存在则上传，若存在和文件一致则跳过，若不一致则更新
// 默认读取配置文件 ~/.config/baidu-pan-rs/config.toml
// 配置文件中包含百度网盘的token 百度盘的根目录id 本地文件的根目录

mod auth;
mod cli;
mod config;
mod sync;

use crate::auth::{device_auth_with_dns, first_app_use, renew_token};
use crate::cli::{CommandLineArgs, Commands};
use crate::config::{config_load_or_init, get_config_file_path, save_or_update_config, Config};
use baidu_pcs_rs_sdk::baidu_pcs_sdk::pcs::BaiduPcsClient;
use baidu_pcs_rs_sdk::baidu_pcs_sdk::BaiduPcsApp;
use byte_unit::UnitType;
use chrono::Local;
use clap::Parser;
use log::info;
use simplelog::{Config as LogConfig, LevelFilter, WriteLogger};
use std::fs::File;
use std::{env, fs};

pub(crate) const BAIDU_PCS_APP: BaiduPcsApp = BaiduPcsApp {
    app_name: env!("BAIDU_PCS_APP_NAME"),
    app_key: env!("BAIDU_PCS_APP_KEY"),
    app_secret: env!("BAIDU_PCS_APP_SECRET"),
    app_id: option_env!("BAIDU_PCS_APP_ID"),
};
fn main() {
    let cli = CommandLineArgs::parse();
    // 日志文件初始化
    let mut log_dir = env::temp_dir();
    log_dir.push("baidu-pcs-rs/logs");
    if !log_dir.exists() {
        fs::create_dir_all(&log_dir).expect("无法创建日志目录");
    }
    let now = Local::now();
    let pid = std::process::id();
    let log_file_name = format!("{}-{}.log", now.format("%Y%m%dT%H%M%S"), pid);
    let log_file_path = log_dir.join(log_file_name);
    // 初始化日志级别：默认根据编译模式决定（debug 构建为 Debug），命令行 --debug 或 -v 系列参数可覆盖
    let mut log_level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    // 支持全局 -v -vv -vvv （组合形式或重复出现），优先级高于编译模式
    let v_count: usize = env::args()
        .skip(1)
        .filter(|a| a.starts_with('-') && a.chars().skip(1).all(|c| c == 'v'))
        .map(|a| a.chars().skip(1).filter(|&c| c == 'v').count())
        .sum();
    if v_count > 0 {
        log_level = match v_count {
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            _ => LevelFilter::Trace, // -vvv 及以上视为最详尽的 Trace
        };
    }
    let log_file = File::create(&log_file_path).expect("无法创建日志文件");
    WriteLogger::init(log_level, LogConfig::default(), log_file).expect("日志初始化失败");

    // version 子命令无需配置和认证，直接输出版本信息
    if matches!(cli.command, Some(Commands::Version)) {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    // 检查配置文件是否存在，如果不存在说明是第一次使用， 提示用户
    let path = get_config_file_path(cli.config.as_ref());
    if !path.exists() {
        if !first_app_use() {
            return;
        }
    }

    // 加载配置（传递 CLI 指定的 DNS，用于首次认证和默认写入配置）
    let mut config: Config =
        config_load_or_init(cli.config.as_ref(), None, None, cli.dns.as_deref());

    if config.is_need_refresh_token() {
        info!("Access token (即将)过期，正在刷新...");
        // Clone DNS options first to avoid borrowing from `config` while passing `&mut config`.
        let dns_opt_owned: Option<String> = config.dns.clone().or(cli.dns.clone());
        renew_token(&mut config, cli.config.as_ref(), dns_opt_owned.as_deref());
        info!("Access token 刷新成功");
    }
    let mut client: BaiduPcsClient = BaiduPcsClient::new_with_dns(
        config.baidu_pan.access_token.as_str(),
        BAIDU_PCS_APP,
        config.dns.as_deref().or(cli.dns.as_deref()),
    );
    match client.ware() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{}", String::from(e));
            return;
        }
    }
    match &cli.command {
        Some(Commands::Auth) => {
            if !config.baidu_pan.access_token.is_empty() && !config.is_need_refresh_token() {
                let client = BaiduPcsClient::new_with_dns(
                    config.baidu_pan.access_token.as_str(),
                    BAIDU_PCS_APP,
                    config.dns.as_deref().or(cli.dns.as_deref()),
                );
                if let Ok(info) = client.get_user_info() {
                    println!("当前登录凭证 {} {} ({})仍然有效，无需重新认证。如需切换账号可另外指定 --config 参数切换账号配置", info.baidu_name() ,info.netdisk_name(), match info.vip_type() {
                        0 => "普通用户".to_string(),
                        1 => "普通会员".to_string(),
                        2 => "超级会员".to_string(),
                        _ => "未知会员类型".to_string(),
                    });
                    return;
                }
            }
            println!("执行认证授权...");
            let token = device_auth_with_dns(config.dns.as_deref().or(cli.dns.as_deref()));
            config.update_token(token);
            save_or_update_config(&mut config, None);
        }
        Some(Commands::Rx(args)) => {
            println!(
                "下载: {} -> {}",
                args.remote,
                args.local.as_deref().unwrap_or(".")
            );
            sync::run_download_task(args, &config, &client);
        }
        Some(Commands::Tx(args)) => {
            println!("上传: {} -> {}", args.local, args.remote);
            sync::run_upload_task(args, &config, &client);
        }
        Some(Commands::Ls(args)) => {
            println!("列出网盘文件: {:?} 递归: {}", args.remote, args.recursive);
            let list = client.list_dir(args.remote.as_str());
            match list {
                Ok(files) => {
                    if files.list().is_empty() {
                        println!("目录为空");
                        return;
                    }
                    for file in files.list() {
                        println!(
                            "{}\t{}\t{}\t{} \t {}",
                            if *file.is_dir() == 1 { "d" } else { "-" },
                            file.size(),
                            file.server_filename(),
                            file.path(),
                            file.fs_id()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("列出文件失败: {}", e);
                }
            }
        }
        Some(Commands::Rm(args)) => {
            println!("即将删除网盘文件: {:?}", args.remote);
            let result = client.delete(&args.remote, Some(false));
            match result {
                Ok(res) => {
                    println!("删除成功: {:?}", res);
                }
                Err(e) => {
                    eprintln!("删除失败: {}", e);
                }
            }
        }
        Some(Commands::Cp(args)) => {
            println!("复制: {} -> {}", args.src, args.dest);
            match client.copy_file(&args.src, &args.dest) {
                Ok(res) => println!("复制成功: {:?}", res),
                Err(e) => eprintln!("复制失败: {}", e),
            }
        }
        Some(Commands::Mv(args)) => {
            println!("移动: {} -> {}", args.src, args.dest);
            match client.move_file(&args.src, &args.dest) {
                Ok(res) => println!("移动成功: {:?}", res),
                Err(e) => eprintln!("移动失败: {}", e),
            }
        }
        Some(Commands::Backup(args)) => {
            println!("备份: {} -> {}", args.local, args.remote);
            sync::run_backup_task(args, &client);
        }
        Some(Commands::Wget(args)) => {
            println!(
                "分享下载: {} -> {}",
                args.share_url,
                args.output.as_deref().unwrap_or(".")
            );
            sync::run_wget_task(args, &client);
        }
        Some(Commands::Mkdir(args)) => {
            for remote_path in &args.remote {
                println!("创建目录: {}", remote_path);
                match client.create_folder(remote_path) {
                    Ok(res) => {
                        println!("✓ 创建成功: {}", remote_path);
                        println!("  路径: {}", res.path());
                    }
                    Err(e) => {
                        let err_msg = e.to_string();
                        if args.parents && err_msg.contains("父目录不存在")
                            || err_msg.contains("110")
                        {
                            eprintln!("✗ 创建失败 (父目录不存在): {}", remote_path);
                        } else if err_msg.contains("文件已存在")
                            || err_msg.contains("已存在")
                            || err_msg.contains("112")
                        {
                            println!("⊘ 目录已存在: {}", remote_path);
                        } else {
                            eprintln!("✗ 创建失败: {}", e);
                        }
                    }
                }
            }
        }
        Some(Commands::Version) => unreachable!("已在前面提前处理"),
        Some(Commands::Quota(args)) => match client.get_user_quota(true, true) {
            Ok(quota) => {
                let total = *quota.total();
                let used = *quota.used();
                let free = *quota.free();
                let idle = total - used + free;

                let print_human = |v: u64| {
                    let adj = byte_unit::Byte::from_u64(v).get_appropriate_unit(UnitType::Binary);
                    format!("{:.3} {}", adj.get_value(), adj.get_unit())
                };

                if args.human {
                    println!(
                        "总空间: {}, 已用: {}, 免费空间: {}, 空闲空间: {}",
                        print_human(total),
                        print_human(used),
                        print_human(free),
                        print_human(idle)
                    );
                } else {
                    let (unit, div): (&str, f64) = if args.gb {
                        ("GB", 1024f64 * 1024f64 * 1024f64)
                    } else if args.mb {
                        ("MB", 1024f64 * 1024f64)
                    } else if args.kb {
                        ("KB", 1024f64)
                    } else {
                        ("B", 1.0)
                    };

                    let fmt = |v: u64| -> String {
                        if div == 1.0 {
                            format!("{} {}", v, unit)
                        } else {
                            format!("{:.3} {}", v as f64 / div, unit)
                        }
                    };

                    println!(
                        "总空间: {}, 已用: {}, 免费空间: {}, 空闲空间: {}",
                        fmt(total),
                        fmt(used),
                        fmt(free),
                        fmt(idle)
                    );
                }
            }
            Err(app) => {
                eprintln!("{}", app)
            }
        },
        None => {
            //TODO 进入 shell 交互 可以 ls mv rename rm upload download
        }
    }
}
