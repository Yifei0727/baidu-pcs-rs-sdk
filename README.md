# baidu-pcs-rs-sdk / baidu-pcs-cli-rs

- 许可协议: MIT
- 平台: macOS / Linux / Windows（需 Rust 稳定版）
- 仓库: https://github.com/yifei0727/baidu-pcs-rs-sdk

1. 项目介绍

    - 本项目同时提供 Rust SDK 与命令行工具（单个 crate 内包含 lib 与 bin）：
        - 库 crate: baidu-pcs-rs-sdk（lib 名: baidu_pcs_rs_sdk），封装百度网盘开放平台接口，支持认证、配额查询、目录列表、分片上传/下载、删除等。
        - 可执行文件: baidu-pcs-cli-rs（CLI），基于 SDK 实现的跨平台命令行工具。
    - 能力概览：
        - 设备码授权（Device Code）登录并持久化 token 到配置文件
        - 查询网盘容量配额
        - 列出目录内容
        - 上传（支持大文件分片，进度条）与下载
        - 删除文件/目录（支持递归）

2. 用户使用说明

   2.1 安装与环境变量

    - 先在百度网盘开放平台创建“我的应用”，获得 AppKey 与 Secret。 https://pan.baidu.com/union/console/createapp

    - 在构建或安装前，需在“编译时”提供以下环境变量（env! 宏会在编译期读取，否则无法编译）：

        - `BAIDU_PCS_APP_NAME`: 应用名称
        - `BAIDU_PCS_APP_KEY`: AppKey
        - `BAIDU_PCS_APP_SECRET`: SecretKey

      > 也可以取网上搜（Github源码等）已有的公开应用，但不保证长期可用。
      > 但强烈建议自己创建应用，避免因他人应用被封禁而无法使用。且不能保证他人应用的安全性，请谨慎使用。
      > ** 此程序开源且可自行编译，凭据和数据、日志仅本地存储和使用。**
      > > github 上某个仓库提供的凭据
      > 如 https://github.com/oott123/bpcs_uploader/blob/master/_bpcs_files_/core.php
      > * (Mac/Linux 示例)
      > ```bash
      > export BAIDU_PCS_APP_NAME="bpcs_uploader"
      > export BAIDU_PCS_APP_KEY="uFBSHEwWE6DD94SQx9z77vgG"
      > export BAIDU_PCS_APP_SECRET="7w6wdSFsTk6Vv586r1W1ozHLoDGhXogD"
      > cargo install baidu-pcs-rs-sdk
      > ```
      > * Windows (PowerShell) 示例
      > ```powershell
      > set BAIDU_PCS_APP_NAME="bpcs_uploader"
      > set BAIDU_PCS_APP_KEY="uFBSHEwWE6DD94SQx9z77vgG"
      > set BAIDU_PCS_APP_SECRET="7w6wdSFsTk6Vv586r1W1ozHLoDGhXogD"
      > cargo install baidu-pcs-rs-sdk
      > ```

    - 安装方式：

        - 使用 cargo 安装（示例）: 先导出上述环境变量后，再执行 cargo install baidu-pcs-rs-sdk
        - 或在源码仓库中执行：先导出环境变量，再 cargo build --release

    - 生成的可执行文件名为 baidu-pcs-cli-rs（注意：CLI 帮助中展示的程序名为 baidu-pan-cli-rs，为显示名，与可执行文件名不同）。

2.2 首次运行与配置

    - 首次运行会提示进行设备码授权：执行 baidu-pcs-cli-rs auth，按提示在浏览器打开授权链接并输入验证码。
    - 配置文件位置（Linux 默认）：~/.config/baidu-pcs-rs/config.toml
    - 可通过 --config 指定自定义路径。
    - 日志：写入系统临时目录下的 baidu-pcs-rs/logs/{时间-进程号}.log。

2.3 命令与参数

       整体使用 unix 风格，支持子命令与参数缩写。支持如 `ls` `rm` `du` `df` `mv` `cp` 等常用命令别名。

> 例： 常见的操作
> * 列出云盘文件`baidu-pcs-cli-rs ls /`
> * 上传本地文件到云盘 `baidu-pcs-cli-rs up -l ./file.txt -r /data/backup/file.txt`
> * 下载云盘文件到当前目录 `baidu-pcs-cli-rs dl /data/backup/file.txt`
> * 删除云盘文件 `baidu-pcs-cli-rs rm /data/backup/file.txt`

    - 通用参数：
        - --config: 指定配置文件路径
        - --debug: 开启 debug 日志
    - 子命令：
        - `auth`（别名: `login`）: 进行设备码授权并保存 token
        - `quota`（别名: `df`, `du`）: 显示容量配额
            - -H/--human，或 -k/--kb，-m/--mb，-g/--gb 控制单位
        - `list` <remote>（别名: `ls`）: 列出目录内容
            - --recursive 递归列出
        - `upload`（别名: `up` `sz`）: 上传/备份
            - -l/--local <path> 本地路径（文件或目录），默认 /data/backup/
            - -r/--remote <path> 网盘路径，默认 /
            - --recursive 目录时递归（默认关）
            - -K/--include-prefix 当指定 -l 时，是否保留本地路径前缀拼接到远端
        - `download`（别名: `dl` `rz`）: 下载
            - --recursive 当 remote 为目录时递归下载
            - <remote> 远端路径
            - [--local <path>] 本地保存目录（不指定则按文件名保存到当前目录）
        - `remove`（别名: `rm`）: 删除
            - <remote> 远端路径
            - --recursive 递归删除目录

提示与限制：

    * 小文件上传 upload_single_file 受路径限制：仅允许 /apps/{app-name}/ 前缀；CLI 内的大文件上传（upload_large_file）不受该限制。
    * 下载目录时需加 --recursive，否则只尝试按文件处理。

3. 开发者调用说明（Rust SDK）

   3.1 添加依赖

    - 在你的 Cargo.toml 中添加：baidu-pcs-rs-sdk = "0.1.0"

   3.2 初始化与认证

    - 认证推荐走设备码授权流程，得到 PcsAccessToken 后持久化；SDK 侧核心构造：
        - BaiduPcsApp { app_key, app_secret, app_name }（均为 'static 字符串）
        - BaiduPcsClient::new(access_token, app)
        - client.ware() 会预拉取用户信息与配额信息

   3.3 常用 API 速览

    - 用户与配额
        - get_user_info() -> PcsUserInfo
        - get_user_quota(check_free: bool, check_expire: bool) -> PcsDiskQuota
    - 目录与文件
        - list_dir(path: &str) -> PcsFileListResult
        - create_folder(path: &str) -> PcsCreateFolderResult
        - delete(paths: &Vec<String>, is_async: Option<bool>) -> PcsFileTaskOperationResult
    - 上传
        - upload_single_file(local: &str, remote: &str, ondup: i8) -> PcsFileUploadResult
            - 仅允许 /apps/{app-name}/ 路径前缀
        - upload_large_file(local: &str, remote: &str, policy: PcsUploadPolicy, progress_cb) -> PcsFileUploadResult
            - 支持大文件分片上传，不受 /apps 路径限制；提供进度回调（已上传字节/总字节/当前分片等）
    - 下载
        - down_file(remote: &str, local: &str, progress_cb) -> Result<(), AppError>
        - down_file_by_id(fs_id: u64, local: &str, progress_cb) -> Result<(), AppError>
    - 其他
        - get_apps_path() -> /apps/{app-name}

   3.4 错误与类型

    - 统一错误: AppError，包含 error_type(AppErrorType: Network/Server/Client/Unknown)、message、errno。
    - 平台错误: PcsApiError（errno 非 0 表示失败，err_msg 为描述）。
    - 令牌: PcsAccessToken，提供 is_expired / is_need_refresh 等辅助方法。

   3.5 最小示例（伪代码）

    - 获取 access_token（建议设备码授权后持久化到配置文件）。
    - 初始化：
        - let app = BaiduPcsApp { app_key: ..., app_secret: ..., app_name: ... };
        - let mut client = BaiduPcsClient::new(access_token, app);
        - client.ware()?;
    - 列目录：client.list_dir("/")?
    - 上传大文件：client.upload_large_file("./a.bin", "/a.bin", PcsUploadPolicy::Overwrite, |p| { /* 进度 */ })?
    - 下载：client.down_file("/a.bin", "./a.bin", None)?

4. 配置文件说明（${用户配置目录}/baidu-pcs-rs/config.toml）

       程序会在首次运行时创建并写入授权结果，后续命令均默认读取该文件。 不同平台的用户配置目录不同。
       
       - Linux: $XDG_CONFIG_HOME 或 ~/.config
       - macOS: ~/Library/Application Support
       - Windows: {FOLDERID_RoamingAppData}（通常为 C:\Users\{用户}\AppData\Roaming）

5. 日志与调试

    - 日志路径: {系统临时目录}/baidu-pcs-rs/logs
    - --debug 可开启更详细的日志（同时建议设置 RUST_LOG）。

6. 常见问题

    - 编译时报 “环境变量未定义”：需在编译/安装前导出 BAIDU_PCS_APP_NAME/KEY/SECRET。
    - 小文件上传 31064 错误：请将目标路径置于 /apps/{app-name}/ 下，或改用大文件分片上传接口。
    - 目录下载失败：请添加 --recursive。

