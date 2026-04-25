---
name: baidu-pcs-quota
description: 查询百度网盘的磁盘配额信息，显示总空间、已用空间、免费空间和空闲空间，支持多种单位格式。当用户需要了解网盘剩余容量、存储使用情况时激活此技能。
---

# 百度网盘磁盘配额查询

## 使用场景

- 查看网盘总容量和剩余空间
- 确认是否有足够空间执行上传操作
- 以特定单位（KB/MB/GB）查看容量数值

## 命令格式

```bash
baidu-pcs-cli-rs quota [-H] [-k] [-m] [-g] [-v]
# 别名
baidu-pcs-cli-rs df
baidu-pcs-cli-rs du
```

## 参数说明

| 参数 | 说明 |
|------|------|
| `-H` / `--human` | 自动选择合适单位（如 GiB、TiB），人类可读格式 |
| `-k` / `--kb` | 以 KB 为单位显示 |
| `-m` / `--mb` | 以 MB 为单位显示 |
| `-g` / `--gb` | 以 GB 为单位显示 |
| `-v` / `--verbose` | 显示详细信息 |

> 注意：`-H`、`-k`、`-m`、`-g` 四个参数互斥，只能选其一

## 输出字段

- **总空间**：网盘账户的总容量
- **已用**：当前已占用的空间
- **免费空间**：运营商赠送的免费额度
- **空闲空间**：实际可用的剩余空间

## 示例

```bash
# 以字节显示（默认）
baidu-pcs-cli-rs quota

# 人类可读格式
baidu-pcs-cli-rs quota -H

# 以 GB 显示
baidu-pcs-cli-rs df -g

# 以 MB 显示
baidu-pcs-cli-rs du -m
```
