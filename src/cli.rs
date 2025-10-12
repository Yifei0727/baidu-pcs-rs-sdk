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
    /// 下载文件
    #[command(alias = "dl", alias = "rx")]
    Download(DownloadArgs),
    /// 上传文件
    #[command(alias = "up", alias = "tx")]
    Upload(UploadArgs),
    /// 列出网盘文件
    #[command(alias = "ls")]
    List(ListArgs),
    /// 删除网盘文件
    #[command(alias = "rm", alias = "del")]
    Remove(RemoveArgs),
    /// 显示磁盘配额
    #[command(alias = "df", alias = "du")]
    Quota(DiskQuotaArgs),
}

/// 上传（备份）命令参数
#[derive(Args)]
pub struct UploadArgs {
    /// 如果是目录，是否递归下载，默认 false
    #[arg(long = "recursive", default_value = "false", action = ArgAction::SetTrue)]
    pub recursive: bool,
    /// 要备份的本地文件夹路径
    /// 可选 默认本地 /data/backup/
    #[arg(short = 'l', long = "local")]
    pub local: Option<String>,
    /// 备份到百度网盘的路径 可选 默认 /
    /// 如果不存在则创建。
    /// 即默认将 本地 /data/backup/ 备份到百度网盘的 /data/backup/
    #[arg(short = 'r', long = "remote")]
    pub remote: Option<String>,
    /// 是否包含前缀
    /// 默认 false
    #[arg(short = 'K', default_value = "false", action = ArgAction::SetTrue)]
    pub include_prefix: bool,
}

/// 下载命令参数
#[derive(Args)]
pub struct DownloadArgs {
    /// 如果是目录，是否递归下载，默认 false
    #[arg( long = "recursive", default_value = "false", action = ArgAction::SetTrue)]
    pub(crate) recursive: bool,
    /// 网盘文件路径
    pub(crate) remote: String,
    /// 本地保存路径
    pub(crate) local: Option<String>,
}

#[derive(Args)]
pub struct ListArgs {
    /// 网盘路径 可选 默认 /
    pub(crate) remote: String,
    /// 是否递归列出子目录
    #[arg(long = "recursive", default_value = "false", action = ArgAction::SetTrue)]
    pub(crate) recursive: bool,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// 网盘路径
    pub(crate) remote: String,
    /// 是否递归删除子目录
    #[arg(long = "recursive", default_value = "false", action = ArgAction::SetTrue)]
    pub(crate) recursive: bool,
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
