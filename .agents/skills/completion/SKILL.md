---
name: baidu-pcs-completion
description: 为百度网盘 CLI 工具生成 shell 自动补全脚本，支持 bash/zsh/fish/powershell 等主流 shell，可安装到 shell 配置文件中。当用户需要启用命令行 Tab 补全功能时激活此技能。
---

# 百度网盘 Shell 补全脚本

## 使用场景

- 启用命令行 Tab 自动补全
- 将补全脚本安装到 shell 配置文件中持久生效

## 命令格式

```bash
baidu-pcs-cli-rs completion [-s <shell>] [-i] [-y]
```

## 参数说明

| 参数 | 类型 | 说明 | 示例 |
|------|------|------|------|
| `-s` / `--shell` | 可选 | 指定 shell 类型（bash/zsh/fish/powershell/elvish），默认自动检测 | `-s zsh` |
| `-i` / `--install` | 可选 | 将补全脚本安装到 shell 配置文件中 | `-i` |
| `-y` / `--yes` | 可选 | 跳过确认提示，非交互模式 | `-y` |

## 注意事项

- 不指定 `--shell` 时会自动检测当前 shell 类型
- 使用 `--install` 可将补全脚本写入 shell 配置文件（如 ~/.bashrc、~/.zshrc），重启 shell 后生效
- `--yes` 配合 `--install` 使用，跳过安装确认提示

## 示例

```bash
# 生成当前 shell 的补全脚本（输出到终端）
baidu-pcs-cli-rs completion

# 生成 zsh 补全脚本
baidu-pcs-cli-rs completion -s zsh

# 安装补全脚本到 shell 配置文件
baidu-pcs-cli-rs completion --install

# 非交互安装（跳过确认）
baidu-pcs-cli-rs completion --install --yes
```
