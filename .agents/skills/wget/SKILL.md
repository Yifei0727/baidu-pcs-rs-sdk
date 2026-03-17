---
name: baidu-pan-wget
description: 下载百度网盘分享链接中的文件到本地，支持需要提取码的分享链接，可指定本地保存目录。当用户需要从百度网盘分享链接下载文件时激活此技能。
---

# 百度网盘分享链接下载

## 使用场景

- 下载他人分享的百度网盘文件
- 保存带提取码的分享链接内容到本地
- 将分享文件保存到指定本地目录

## 命令格式

```bash
baidu-pan-cli-rs wget <分享链接> [-p <提取码>] [-o <本地目录>]
```

## 参数说明

| 参数 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `<分享链接>` | 必填 | 百度网盘分享 URL | `https://pan.baidu.com/s/1xxxxx` |
| `-p` / `--password` | 可选 | 分享提取码（如有） | `-p abcd` |
| `-o` / `--output` | 可选 | 本地保存目录，默认当前目录（`.`） | `-o ~/下载/` |

## 注意事项

- 分享链接格式通常为 `https://pan.baidu.com/s/<ID>`
- 若分享设置了提取码，必须通过 `-p` 参数提供
- 下载文件保存在 `--output` 指定目录下，文件名由分享内容决定

## 示例

```bash
# 下载无提取码的分享链接
baidu-pan-cli-rs wget https://pan.baidu.com/s/1abcdef1234567

# 下载带提取码的分享链接
baidu-pan-cli-rs wget https://pan.baidu.com/s/1abcdef1234567 -p xk9f

# 下载到指定目录
baidu-pan-cli-rs wget https://pan.baidu.com/s/1abcdef1234567 -p xk9f -o ~/下载/分享文件
```
