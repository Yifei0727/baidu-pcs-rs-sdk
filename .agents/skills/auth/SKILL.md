---
name: baidu-pcs-auth
description: 对百度网盘 CLI 工具（baidu-pcs-cli-rs）执行 OAuth 设备码授权登录，或检查当前登录状态是否有效。当用户需要登录百度网盘、切换账号或授权认证时激活此技能。
---

# 百度网盘认证授权

## 使用场景

- 首次使用工具，尚未完成授权
- 当前 Token 已失效，需要重新登录
- 切换到其他百度账号

## 命令格式

```bash
baidu-pcs-cli-rs auth [--config <配置文件路径>] [--dns <DNS服务器>]
# 别名
baidu-pcs-cli-rs login
```

## 执行步骤

1. 运行 `baidu-pcs-cli-rs auth` 命令
2. 终端会输出一个设备码和授权 URL（格式如 `https://openapi.baidu.com/device`）
3. 在浏览器中打开该 URL，输入设备码，用百度账号完成扫码或登录授权
4. 授权完成后，工具自动将 Token 写入配置文件（默认 `~/.config/baidu-pcs-rs/config.toml`），权限为 0o600

## 参数说明

| 参数 | 说明 | 示例 |
|------|------|------|
| `--config` | 指定配置文件路径，用于多账号切换 | `--config ~/work-account.toml` |
| `--dns` | 指定自定义 DNS 服务器（逗号分隔，支持 IP 或 IP:PORT） | `--dns 8.8.8.8,1.1.1.1:53` |

## 注意事项

- 若当前 Token 仍有效，工具会输出当前账号信息并提示无需重新认证
- 如需切换账号，使用 `--config` 指定不同的配置文件路径
- Token 过期后下次执行任何命令会自动尝试刷新，无需手动重新 auth

## 示例

```bash
# 标准登录
baidu-pcs-cli-rs auth

# 使用指定配置文件（多账号场景）
baidu-pcs-cli-rs auth --config ~/.config/baidu-pcs-rs/work.toml
```
