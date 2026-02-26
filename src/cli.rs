use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "baidu-pan-cli-rs")]
#[command(about = "百度网盘命令行工具", long_about = None)]
pub struct CommandLineArgs {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// 配置文件路径， 可选 默认  用户应用配置目录下的 baidu-pan-rs/config.toml
    #[arg(short, long, default_value = None)]
    pub config: Option<String>,

    /// 是否开启 debug 日志
    #[arg(long, action = ArgAction::SetTrue)]
    pub debug: bool,

    /// 指定用于解析域名的 DNS 服务器地址（支持逗号分隔多个，格式如 8.8.8.8 或 8.8.8.8:53）
    #[arg(long, default_value = None)]
    pub dns: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 认证授权
    #[command(alias = "login")]
    Auth,
    /// 列出远程目录
    #[command(alias = "list")]
    Ls(LsArgs),
    /// 复制远程文件/目录
    #[command(alias = "copy")]
    Cp(CpArgs),
    /// 移动/重命名远程文件/目录
    #[command(alias = "rename")]
    Mv(MvArgs),
    /// 删除远程文件/目录
    #[command(alias = "del", alias = "remove")]
    Rm(RmArgs),
    /// 上传（本地 → 远程）
    #[command(alias = "upload", alias = "up")]
    Tx(TxArgs),
    /// 下载（远程 → 本地）
    #[command(alias = "download", alias = "dl")]
    Rx(RxArgs),
    /// 备份（仅上传远程不存在的文件）
    Backup(BackupArgs),
    /// 显示磁盘配额
    #[command(alias = "df", alias = "du")]
    Quota(DiskQuotaArgs),
}

/// ls <remote> [-r]
#[derive(Args)]
pub struct LsArgs {
    /// 远程路径
    pub remote: String,
    /// 递归列出子目录
    #[arg(short = 'r', long = "recursive", action = ArgAction::SetTrue)]
    pub recursive: bool,
}

/// cp <src> <dest>  （远程 → 远程）
#[derive(Args)]
pub struct CpArgs {
    /// 远程源路径
    pub src: String,
    /// 远程目标路径
    pub dest: String,
}

/// mv <src> <dest>  （远程 → 远程）
#[derive(Args)]
pub struct MvArgs {
    /// 远程源路径
    pub src: String,
    /// 远程目标路径
    pub dest: String,
}

/// rm <remote>... [-r]
#[derive(Args)]
pub struct RmArgs {
    /// 远程路径（支持多个）
    #[arg(required = true)]
    pub remote: Vec<String>,
    /// 递归删除子目录
    #[arg(short = 'r', long = "recursive", action = ArgAction::SetTrue)]
    pub recursive: bool,
}

/// tx <local> <remote> [-r] [--remove-source]
#[derive(Args)]
pub struct TxArgs {
    /// 本地源路径
    pub local: String,
    /// 远程目标路径
    pub remote: String,
    /// 递归上传目录
    #[arg(short = 'r', long = "recursive", action = ArgAction::SetTrue)]
    pub recursive: bool,
    /// 上传完成后删除本地源文件
    #[arg(long = "remove-source", action = ArgAction::SetTrue)]
    pub remove_source: bool,
}

/// rx <remote> [local] [-r]
#[derive(Args)]
pub struct RxArgs {
    /// 远程源路径
    pub remote: String,
    /// 本地目标路径（默认当前目录）
    pub local: Option<String>,
    /// 递归下载目录
    #[arg(short = 'r', long = "recursive", action = ArgAction::SetTrue)]
    pub recursive: bool,
}

/// backup <local> <remote> [--remove-source]
#[derive(Args)]
pub struct BackupArgs {
    /// 本地源目录/文件
    pub local: String,
    /// 远程目标目录
    pub remote: String,
    /// 上传完成后删除本地源文件
    #[arg(long = "remove-source", action = ArgAction::SetTrue)]
    pub remove_source: bool,
}

#[derive(Args)]
pub struct DiskQuotaArgs {
    /// 是否显示详细信息
    #[arg(short = 'v', long = "verbose", default_value = "false", action = ArgAction::SetTrue)]
    pub verbose: bool,

    #[arg(short = 'H', long="human",conflicts_with_all = &["kb", "mb", "gb"])]
    pub human: bool,
    #[arg(short = 'k', long="kb",conflicts_with_all = &["human", "mb", "gb"])]
    pub kb: bool,
    #[arg(short = 'm', long="mb", conflicts_with_all = &["human", "kb", "gb"])]
    pub mb: bool,
    #[arg(short = 'g', long="gb", conflicts_with_all = &["human", "kb", "mb"])]
    pub gb: bool,
}
