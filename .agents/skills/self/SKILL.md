---
name: baidu-pcs-self
description: 管理百度网盘 CLI 工具自身，包括查看配置文件路径、检查更新和自动下载最新版本。当用户需要查看配置文件位置、检查或升级工具版本时激活此技能。
---

# 百度网盘应用自管理

## 使用场景

- 查看当前配置文件路径
- 检查是否有新版本可用
- 自动下载当前平台架构的最新版本

## 命令格式

```bash
baidu-pcs-cli-rs app self <子命令>
# 别名
baidu-pcs-cli-rs self <子命令>
```

## 子命令

| 子命令 | 别名 | 说明 |
|--------|------|------|
| `config` | `cfg` | 显示当前配置文件路径 |
| `update` | `up` | 检查更新 / 下载最新版本 |

### self update 参数

| 参数 | 说明 |
|------|------|
| `--dry-run` | 只检查是否有新版本，不执行更新 |
| `--download [目录]` | 下载最新版本到指定目录（不提供则下载到当前目录；若提供的是文件路径则提示需要目录路径） |

## 示例

```bash
# 查看配置文件路径
baidu-pcs-cli-rs self config

# 检查更新
baidu-pcs-cli-rs self update

# 仅检查是否有新版本（不执行更新）
baidu-pcs-cli-rs self update --dry-run

# 下载最新版本到当前目录
baidu-pcs-cli-rs self update --download

# 下载最新版本到指定目录
baidu-pcs-cli-rs self update --download ~/下载/
```
