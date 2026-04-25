// 扫描输入的文件/文件夹 并于百度网盘中的文件/文件夹进行比对
// 若不存在则上传，若存在和文件一致则跳过，若不一致则更新
// 默认读取配置文件 ~/.config/baidu-pan-rs/config.toml
// 配置文件中包含百度网盘的token 百度盘的根目录id 本地文件的根目录

mod auth;
mod cli;
mod config;
mod sync;

use crate::auth::{device_auth_with_dns, first_app_use, renew_token};
use crate::cli::{CommandLineArgs, Commands, CompletionArgs, SelfCommand};
use crate::config::{config_load_or_init, get_config_file_path, save_or_update_config, Config};
use baidu_pcs_rs_sdk::baidu_pcs_sdk::pcs::BaiduPcsClient;
use baidu_pcs_rs_sdk::baidu_pcs_sdk::BaiduPcsApp;
use byte_unit::UnitType;
use chrono::Local;
use clap::{CommandFactory, Parser};
use clap_complete::Shell;
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
fn check_for_update(dry_run: bool) {
    let current = env!("CARGO_PKG_VERSION");
    // 优先从 GitHub CI 环境变量构建 API 地址，本地构建则回退到 Cargo.toml repository
    let api_base = option_env!("GITHUB_API_URL").unwrap_or("https://api.github.com");
    let repo = option_env!("GITHUB_REPOSITORY").unwrap_or_else(|| {
        // 回退：从 CARGO_PKG_REPOSITORY 解析出 owner/repo
        env!("CARGO_PKG_REPOSITORY")
            .trim_start_matches("https://github.com/")
        // 注意：trim_start_matches 返回 &str，但 unwrap_or_else 需要 &&str
        // 这里利用编译期常量拼接的特性，实际上 option_env! 和 env! 都是 &'static str
    });
    let url = format!("{api_base}/repos/{repo}/releases/latest");

    let client = reqwest::blocking::Client::builder()
        .user_agent("baidu-pcs-cli-rs")
        .build();

    match client {
        Ok(c) => match c.get(url).send() {
            Ok(resp) => match resp.json::<serde_json::Value>() {
                Ok(json) => {
                    let latest = json["tag_name"].as_str().unwrap_or("unknown");
                    let latest_trimmed = latest.trim_start_matches('v');
                    if latest_trimmed == current {
                        println!("当前已是最新版本: v{}", current);
                    } else {
                        println!("有新版本可用: {} (当前: v{})", latest, current);
                        if dry_run {
                            return;
                        }
                        if let Some(html_url) = json["html_url"].as_str() {
                            println!("暂不支持自动更新，请前往发布页面: {}", html_url);
                        }
                        // TODO: 非 dry-run 时执行自动更新
                    }
                }
                Err(e) => eprintln!("解析更新信息失败: {}", e),
            },
            Err(e) => eprintln!("检查更新失败: {}", e),
        },
        Err(e) => eprintln!("检查更新失败: {}", e),
    }
}

fn run_completion(args: &CompletionArgs) {
    let shell_name = args.shell.as_deref().unwrap_or_else(|| detect_shell());
    let shell: Shell = match shell_name.to_lowercase().as_str() {
        "bash" => Shell::Bash,
        "zsh" => Shell::Zsh,
        "fish" => Shell::Fish,
        "powershell" | "pwsh" => Shell::PowerShell,
        other => {
            eprintln!("不支持的 shell: {}\n支持的 shell: bash, zsh, fish, powershell", other);
            return;
        }
    };

    if args.install {
        install_completion(shell, args.yes);
    } else {
        clap_complete::generate(
            shell,
            &mut CommandLineArgs::command(),
            "baidu-pcs-cli-rs",
            &mut std::io::stdout(),
        );
    }
}

fn detect_shell() -> &'static str {
    // 优先从 SHELL 环境变量检测
    if let Ok(shell) = env::var("SHELL") {
        if shell.contains("zsh") {
            return "zsh";
        } else if shell.contains("bash") {
            return "bash";
        } else if shell.contains("fish") {
            return "fish";
        }
    }
    // Windows 下检测 PSModulePath 判断 PowerShell
    if env::var("PSModulePath").is_ok() {
        return "powershell";
    }
    // 默认 bash
    "bash"
}

fn install_completion(shell: Shell, skip_confirm: bool) {
    match shell {
        Shell::Zsh => install_completion_zsh(skip_confirm),
        Shell::Bash | Shell::Fish | Shell::PowerShell => {
            install_completion_eval(shell, skip_confirm)
        }
        _ => eprintln!("不支持自动安装该 shell 的补全脚本"),
    }
}

/// zsh: 写入 fpath 目录下的补全文件（避免 eval 时 glob 展开问题）
fn install_completion_zsh(skip_confirm: bool) {
    let comp_dir = dirs_home().join(".zsh/completion");
    let comp_file = comp_dir.join("_baidu-pcs-cli-rs");
    let rc_file = dirs_home().join(".zshrc");

    println!("将补全脚本安装到: {}", comp_file.display());
    println!("并在 {} 中添加 fpath 设置", rc_file.display());
    if !skip_confirm {
        println!("是否继续? [y/N] ");
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err() {
            eprintln!("读取输入失败");
            return;
        }
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("已取消");
            return;
        }
    }

    // 确保补全目录存在
    if let Err(e) = fs::create_dir_all(&comp_dir) {
        eprintln!("创建目录失败: {} - {}", comp_dir.display(), e);
        return;
    }

    // 写入补全文件
    let mut buf = Vec::new();
    clap_complete::generate(
        Shell::Zsh,
        &mut CommandLineArgs::command(),
        "baidu-pcs-cli-rs",
        &mut buf,
    );
    if let Err(e) = fs::write(&comp_file, &buf) {
        eprintln!("写入 {} 失败: {}", comp_file.display(), e);
        return;
    }

    // 在 .zshrc 中添加 fpath 和 compinit（如未添加）
    let fpath_line = r#"fpath=(~/.zsh/completion $fpath)"#;
    let compinit_line = "autoload -Uz compinit && compinit -i";
    if rc_file.exists() {
        if let Ok(content) = fs::read_to_string(&rc_file) {
            let mut to_append = Vec::new();
            if !content.contains("fpath=(") || !content.contains(".zsh/completion") {
                to_append.push(format!("# baidu-pcs-cli-rs zsh completion\n{}", fpath_line));
            }
            // 仅在没有 compinit 时添加
            if !content.contains("compinit") {
                to_append.push(compinit_line.to_string());
            }
            if to_append.is_empty() {
                println!("补全脚本已安装到 {}，.zshrc 无需修改", comp_file.display());
                return;
            }
            if let Err(e) = fs::OpenOptions::new()
                .append(true)
                .open(&rc_file)
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f)?;
                    for line in &to_append {
                        writeln!(f, "{}", line)?;
                    }
                    Ok(())
                })
            {
                eprintln!("写入 {} 失败: {}", rc_file.display(), e);
                return;
            }
        }
    } else {
        // .zshrc 不存在，创建
        if let Err(e) = fs::write(
            &rc_file,
            format!(
                "# baidu-pcs-cli-rs zsh completion\n{}\n{}\n",
                fpath_line, compinit_line
            ),
        ) {
            eprintln!("写入 {} 失败: {}", rc_file.display(), e);
            return;
        }
    }
    println!(
        "补全脚本已安装。请执行 `source ~/.zshrc` 或重新打开终端使其生效"
    );
}

/// bash/fish/powershell: 追加 eval/source snippet 到 rc 文件
fn install_completion_eval(shell: Shell, skip_confirm: bool) {
    let (rc_file, snippet) = match shell {
        Shell::Bash => {
            let rc = dirs_home().join(".bashrc");
            let s = r#"eval "$(baidu-pcs-cli-rs completion)""#.to_string();
            (rc, s)
        }
        Shell::Fish => {
            let rc = dirs_home().join(".config/fish/config.fish");
            let s = r#"baidu-pcs-cli-rs completion | source"#.to_string();
            (rc, s)
        }
        Shell::PowerShell => {
            let profile = if cfg!(target_os = "windows") {
                dirs_home().join("Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1")
            } else {
                dirs_home().join(".config/powershell/Microsoft.PowerShell_profile.ps1")
            };
            let s = r#"Invoke-Expression ($(baidu-pcs-cli-rs completion --shell powershell) | Out-String)"#.to_string();
            (profile, s)
        }
        _ => unreachable!(),
    };

    println!("将补全脚本安装到: {}", rc_file.display());
    if !skip_confirm {
        println!("是否继续? [y/N] ");
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_err() {
            eprintln!("读取输入失败");
            return;
        }
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("已取消");
            return;
        }
    }

    // 如果文件已有相同 snippet 则跳过
    if rc_file.exists() {
        if let Ok(content) = fs::read_to_string(&rc_file) {
            if content.contains(snippet.trim()) {
                println!("补全脚本已存在于 {}，跳过", rc_file.display());
                return;
            }
        }
    }

    // 确保父目录存在
    if let Some(parent) = rc_file.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("创建目录失败: {} - {}", parent.display(), e);
                return;
            }
        }
    }

    // 追加 snippet
    if let Err(e) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_file)
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f)?;
            writeln!(f, "# baidu-pcs-cli-rs shell completion")?;
            writeln!(f, "{}", snippet)?;
            Ok(())
        })
    {
        eprintln!("写入 {} 失败: {}", rc_file.display(), e);
        return;
    }
    println!(
        "补全脚本已安装到 {}，请重新打开终端或 source 该文件使其生效",
        rc_file.display()
    );
}

fn dirs_home() -> std::path::PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

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

    // completion 子命令无需配置和认证
    if let Some(Commands::Completion(args)) = &cli.command {
        run_completion(args);
        return;
    }

    // self 子命令无需配置和认证
    if let Some(Commands::AppSelf(args)) = &cli.command {
        match &args.command {
            SelfCommand::Config => {
                println!("{}", get_config_file_path(cli.config.as_ref()).display());
            }
            SelfCommand::Update(args) => {
                check_for_update(args.dry_run);
            }
        }
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
            if args.daemon {
                println!("备份(守护模式): {} -> {}", args.local, args.remote);
            } else {
                println!("备份: {} -> {}", args.local, args.remote);
            }
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
        Some(Commands::AppSelf(_)) => unreachable!("已在前面提前处理"),
        Some(Commands::Completion(_)) => unreachable!("已在前面提前处理"),
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
