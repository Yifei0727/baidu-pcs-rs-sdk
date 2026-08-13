# 更新日志 / Changelog

本文件记录各版本的可见变更。格式参考 [Keep a Changelog](https://keepachangelog.com/)，版本号遵循语义化版本（SemVer）。

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

[0.4.3]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.4.3
[0.4.2]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.4.2
[0.4.1]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.4.1
[0.4.0]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.4.0
[0.3.0]: https://github.com/Yifei0727/baidu-pcs-rs-sdk/releases/tag/v0.3.0
