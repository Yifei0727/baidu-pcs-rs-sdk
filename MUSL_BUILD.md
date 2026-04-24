# Linux musl 构建支持

本项目支持为 musl libc 的 Linux 发行版编译二进制文件。

## 支持的目标

- **x86_64-unknown-linux-musl**: x86_64 架构 musl
- **aarch64-unknown-linux-musl**: ARM64 架构 musl

## 特点

musl 版本的二进制文件具有以下特点：

- **完全静态链接**: 不依赖系统的 libc，可以在任何 Linux 发行版运行
- **轻量级**: 适用于 Alpine Linux、BusyBox 等最小化发行版
- **独立性**: 不需要安装额外的运行时库

## 本地编译

### 安装编译工具

```bash
# 安装 cross（用于跨平台编译）
cargo install cross

# 或通过 cargo-binstall（更快）
cargo binstall cross
```

### 编译 musl 版本

```bash
# x86_64 musl
cross build --target x86_64-unknown-linux-musl --release

# aarch64 musl
cross build --target aarch64-unknown-linux-musl --release
```

编译产物位于：
- `target/x86_64-unknown-linux-musl/release/baidu-pcs-cli-rs`
- `target/aarch64-unknown-linux-musl/release/baidu-pcs-cli-rs`

## CI/CD 构建

GitHub Actions workflow 自动为以下平台构建：

| 平台 | 构建方式 | 输出 |
|------|--------|------|
| x86_64 GNU | cargo | baidu-pcs-cli-rs-x86_64-unknown-linux-gnu |
| x86_64 musl | cross | baidu-pcs-cli-rs-x86_64-unknown-linux-musl |
| ARM64 GNU | cross | baidu-pcs-cli-rs-aarch64-unknown-linux-gnu |
| ARM64 musl | cross | baidu-pcs-cli-rs-aarch64-unknown-linux-musl |
| Windows | cargo | baidu-pcs-cli-rs-x86_64-pc-windows-msvc.exe |
| macOS ARM | cargo | baidu-pcs-cli-rs-aarch64-apple-darwin |

tag 发布时，所有构件都会自动上传到 GitHub Releases。

## 问题排查

### OpenSSL 编译错误

如果遇到 `openssl-sys` 编译错误，通常是因为：

1. Cross 容器中缺少 OpenSSL 开发库 - 已在 `Cross.toml` 中配置自动安装
2. 环境变量未正确设置 - workflow 中已自动配置 `OPENSSL_DIR` 和 `OPENSSL_STATIC`

### 构建超时

musl 构建通常比 GNU 构建耗时更长，特别是首次构建。可以：

- 本地构建时可能需要 10-20 分钟
- CI 构建通常在 GitHub Actions 的资源限制内完成

## 使用场景

musl 版本适用于：

- Alpine Linux 和其他基于 musl 的发行版
- Docker 镜像（特别是轻量级镜像）
- 最小化的云原生环境
- 需要完整静态链接的环境
