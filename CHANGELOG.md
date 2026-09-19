# 更新日志 / Changelog

本文件记录各版本的可见变更。格式参考 [Keep a Changelog](https://keepachangelog.com/)，版本号遵循语义化版本（SemVer）。

## [0.6.0] - 2026-09-19

### 变更 (Changed)
- **依赖分拆与架构解耦（Core SDK 与 CLI 隔离）**：
  - 将 `clap`、`clap_complete`、`indicatif`、`simplelog`、`directories`、`byte-unit`、`toml` 等命令行专用依赖迁移为可选依赖，统一通过 `cli` feature 管理。
  - 将 `Cargo.toml` 中的默认特性设为空（`default = []`）。当下游项目作为 SDK 引入 `baidu-pcs-rs-sdk = "0.6.0"` 时，默认仅编译纯净 Core SDK，零 CLI 依赖，编译极速轻量。
  - 二进制目标 `baidu-pcs-cli-rs` 设置 `required-features = ["cli"]`；CI 构建与发布的二进制包默认启用 `--all-features`。
- **自定义 DNS 特性解耦与可选化**：
  - 将 `hickory-resolver` 与 `reqwest/hickory-dns` 抽离为独立的 `dns` 可选特性（提供 `custom-dns` 别名）。
  - 默认情况下（未启用 `dns` 特性时）完全不引入 `hickory-resolver`，直接使用操作系统原生 DNS（`getaddrinfo`），彻底消除第三方 SDK 集成（尤其 macOS 后端/沙盒应用）时的 UDP DNS 冲突与限制。
  - 任何平台如需自定义 DNS nameserver 解析，按需显式声明 `features = ["dns"]` 即可正常使用。
- **清理工程冗余与告警**：
  - 移除全工程未使用的废弃依赖 `bytefmt`。
  - 清理各模块中因条件编译与未读字段产生的 `dead_code` 与 `unused_mut` 编译告警。

## [0.5.2] - 2026-09-19

### 修复 (Fixed)
- **文件管理系列接口序列化别名错误 (#1)**：
  - 修复 `DeleteAttributes` 与 `FileManagerAttributes` 误用 `#[serde(alias = "filelist")]` 导致 `delete` / `copy_file` / `move_file` / `rename_file` 实际序列化字段为 `file_list`、API 统一报错 `errno: 2` 的问题；更正为 `#[serde(rename = "filelist")]`。
  - 同步修正 `get_user_quota` 中 `checkfree` / `checkexpire` 以及 `PreCreateAttributes` 中 `content-md5` / `slice-md5` 的序列化命名。
- **大文件上传初始化异常 (#2)**：
  - 修复未显式调用 `ware()` 时直接调用 `upload_large_file` 因 `user_info` 为 `None` 导致的 `unwrap` panic；改为按需动态拉取并兜底默认 4MB 分片大小。
- **网络异常 panic 与 Token 泄露隐患 (#3)**：
  - 消除 `_request` 中的 `send().await.unwrap().text().await`，转为标准 `Result` 传播（`AppErrorType::Network`），避免断网/DNS异常导致崩溃。
  - 对网络异常信息中包含的请求 URL 自动进行 `access_token` 脱敏替换。
  - 消除 `download` 与 `download_range_by_path` 的 `trace!` 日志中拼接的明文 `access_token`。
  - 移除 OAuth 设备授权原始响应报文及 `config.toml` 读取时打印完整 Token 的调试日志。
  - API 错误响应中保留原始 body，并在 `AppError` 中展示对应 `errno` 的中文释义，便于排障。
- **全工程稳定性加固**：
  - 消除认证、文件扫描、本地同步、任务调度、配置读写与日志初始化中的各类潜在 `panic!` / `.unwrap()` / `.expect()`，全面改用标准错误返回与优雅容错。

## [0.5.1] - 2026-09-19

### 变更 (Changed)
- **日志级别调整**：
  - 将底层 HTTP GET / POST 请求路径及响应文本降级为 `trace!` 级别，默认及日常排查下不可见。
  - 将 SDK 客户端被调用的各操作方法（如文件上传、下载、删除、移动、复制、创建目录、配额查询等动作）日志级别设为 `info!`，便于直观追踪具体操作行为。

## [0.5.0] - 2026-09-17

### 新增 (Added)
- **`list_dir` 分页能力**：
  - 新增 `list_dir_paged(path, start, limit)`：分页版列目录，透传 API 的 `start`（起始位置，从 0 开始）与 `limit`（每页条数，建议不超过 1000）参数；原 `list_dir(path)` 签名不变，内部委托本方法。
  - 新增 `list_dir_iter(path, limit) -> PcsDirPager`：自动翻页的同步迭代器（`Iterator<Item = PcsFileItem>`），逐条遍历目录下全部文件，以"返回条数不足一页"判定结束，迭代中途请求出错时静默终止；提供 `into_inner()` 取回客户端引用。

## [0.4.3] - 2026-08-13

### 修复 (Fixed)
- **子命令改名**：`app-self` 改名为 `self`（`app-self` 保留为别名，旧脚本仍可运行）。
- **`self update --download` 平台检测**：原先在 Linux 上始终下载 `x86_64-unknown-linux-gnu` 版本。现按编译期目标（`cfg!(target_env = "musl")`）自动区分 musl / gnu，musl 环境不再误下载 gnu 版二进制。
- **`backup` 批次续传**：非守护模式下，达到 `--backup-batch-files` / `--backup-batch-bytes` 上限后，原先会直接结束并遗留大量未传文件。现改为暂停 `backup-interval` 后继续下一轮，直到全部文件上传完成才退出；只有「本轮新上传数为 0」时才结束。（守护模式行为不变，仍无限循环。）

## [0.4.2] - 2026-08-13

### 新增 (Added)
- **`backup` 文件排序**：新增 `--by-name`（默认）/ `--by-modify`、`--asc`（默认）/ `--desc` 参数。
  - `--by-name`：按相对路径名排序；**同级中文件永远排在子目录之前**（浅目录文件优先，不受 `--asc`/`--desc` 影响），同级名称才受升降序影响；归类顺序 数字(0-9) < 小写(a-z) < 大写(A-Z)。
  - `--by-modify`：按文件修改时间排序（`--asc` = 最近到最远，新→旧；`--desc` = 最远到最近，旧→新）。
  - 扫描后、上传前对文件列表做确定性排序，替换原先依赖 `read_dir` 的乱序。

> 注：备份排序功能原计划随 0.4.1 发布，因漏提交而延至本版本。

## [0.4.1] - 2026-08-13

### 其他 (Changed)
- 仅版本号更新（补发）。实际功能变更见 0.4.2。

## [0.4.0] - 2026-08-12

### 新增 (Added)
- **`backup` 间隔分批上传（规避持续低流量上传拦截）**：
  - `--backup-interval <DUR>`：两次上传会话之间的最小静默间隔，支持 `1h` / `30m` / `3600s` / `1h30m` 等格式，默认 `60` 秒。
  - `--backup-batch-files <N>`：单次会话最多上传文件数（如 `1` 表示每次只传 1 个），未指定则不限制。
  - `--backup-batch-bytes <SIZE>`：单次会话最大流量（如 `1G` / `500M`），达到即结束本轮进入静默间隔，未指定则不限制。
  - 守护模式（`-d`）按「突发上传一批 → 静默间隔」循环，保证网络上传有间隔。
  - `rate_limit` 模块新增 `parse_duration` 解析时间间隔（支持 `s`/`m`/`h`/`d` 多段与空格）。

## [0.3.0] - 2026-08-12

### 新增 (Added)
- **全局上下行限速**：
  - `--band <RATE>`：上下行统一限速（如 `100k` = 100KB/s）。
  - `--tx-band <RATE>`：仅上传限速（如 `1M` = 1MB/s），优先级高于 `--band`。
  - `--rx-band <RATE>`：仅下载限速（如 `100M` = 100MB/s），优先级高于 `--band`。
  - 基于令牌桶算法；`--tx-band`/`--rx-band` 覆盖 `--band`。速率解析支持 `k/K`(1024 基准)、`M`、`G` 及 `KiB/MiB` 等写法。

[0.5.1]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.5.1
[0.5.0]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.5.0
[0.4.3]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.4.3
[0.4.2]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.4.2
[0.4.1]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.4.1
[0.4.0]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.4.0
[0.3.0]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.3.0
