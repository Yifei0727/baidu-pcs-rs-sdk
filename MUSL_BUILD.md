# Linux musl 构建支持 - 计划中

**当前状态**：该功能正在开发中。musl 和 aarch64-gnu 支持目前由于交叉编译环境中的 OpenSSL 依赖问题而临时禁用。

## 支持的目标 (计划)

- **x86_64-unknown-linux-musl**: x86_64 架构 musl
- **aarch64-unknown-linux-musl**: ARM64 架构 musl
- **aarch64-unknown-linux-gnu**: ARM64 架构 GNU (仅本地编译可用)

## 已知问题

### OpenSSL 交叉编译问题

在 cross 容器环境中为 musl 和 aarch64-gnu 编译时，openssl-sys 无法找到 OpenSSL 开发头文件，尽管 libssl-dev 已安装：

```
error: failed to run custom build command for `openssl-sys`
fatal error: openssl/opensslconf.h: No such file or directory
```

**根本原因**：
- 交叉编译容器中 OpenSSL 头文件的位置与编译脚本预期不符
- musl 容器的包管理与 glibc 容器有显著差异
- 某些容器镜像的 libssl-dev 包版本过旧或不完整

**候选解决方案**：
1. 使用 openssl-sys 的 `vendored` feature，让它自己编译 OpenSSL
2. 切换到更新的交叉编译容器基础镜像
3. 为 musl 目标使用完全不同的构建策略（如 Alpine Linux）
4. 考虑使用 rustls 代替 OpenSSL（如果所有依赖都支持）

## 当前支持的平台

GitHub Actions workflow 目前成功构建以下平台：

| 平台 | 构建方式 | 状态 |
|------|--------|------|
| x86_64 GNU Linux | cargo | ✅ 工作 |
| x86_64 Windows | cargo | ✅ 工作 |
| ARM64 macOS | cargo | ✅ 工作 |
| x86_64 musl | cross | ⏸️ 暂停 (OpenSSL 问题) |
| ARM64 GNU Linux | cross | ⏸️ 暂停 (OpenSSL 问题) |
| ARM64 musl | cross | ⏸️ 暂停 (OpenSSL 问题) |

## 本地编译（不适用当前）

等待 OpenSSL 问题解决后会提供完整的本地编译说明。

## 参考资源

- [OpenSSL-sys 文档](https://docs.rs/openssl-sys/)
- [Cross 项目](https://github.com/cross-rs/cross)
- [musl libc](https://musl.libc.org/)
