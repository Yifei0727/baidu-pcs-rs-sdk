---
name: baidu-pan-tx
description: 将本地文件或目录上传到百度网盘（本地 → 远程），支持递归上传目录和上传后删除本地源文件。当用户需要上传文件到网盘时激活此技能。
---

# 百度网盘文件上传

## 使用场景

- 将本地文件上传到网盘
- 将整个本地目录递归上传到网盘
- 上传后自动清理本地文件（移动到云端）

## 命令格式

```bash
baidu-pan-cli-rs tx <本地路径> <远程路径> [-r] [--remove-source]
# 别名
baidu-pan-cli-rs upload <本地路径> <远程路径>
baidu-pan-cli-rs up <本地路径> <远程路径>
```

## 参数说明

| 参数 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `<本地路径>` | 必填 | 本地文件或目录的路径 | `./report.pdf` 或 `~/documents` |
| `<远程路径>` | 必填 | 网盘上的目标路径 | `/我的文件/report.pdf` |
| `-r` / `--recursive` | 可选 | 递归上传目录及其所有内容 | `-r` |
| `--remove-source` | 可选 | 上传成功后删除本地源文件 | `--remove-source` |

## 注意事项

- 上传大文件时使用分块上传，工具会显示进度条
- 上传目录时务必加 `-r` 参数
- `--remove-source` 会在上传成功后删除本地文件，使用前请确认

## 示例

```bash
# 上传单个文件
baidu-pan-cli-rs tx ./report.pdf /我的文件/report.pdf

# 递归上传整个目录
baidu-pan-cli-rs tx ~/documents/项目 /备份/项目 -r

# 上传并删除本地源文件（移动到云端）
baidu-pan-cli-rs tx ~/下载/安装包.zip /软件归档/安装包.zip --remove-source
```
