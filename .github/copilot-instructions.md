# Copilot Instructions — baidu-pcs-rs-sdk

## Build & Test

Three compile-time environment variables are **required** (read via `env!()` macro — build fails without them):

```bash
export BAIDU_PCS_APP_NAME="..."
export BAIDU_PCS_APP_KEY="..."
export BAIDU_PCS_APP_SECRET="..."
```

A `.env.local` file in the repo root contains development values for these variables. Source it or export them manually before building.

```bash
# Build
cargo build --release

# Run all tests
cargo test

# Run a single test
cargo test test_parse_dns_servers_basic

# Cross-compile (e.g. aarch64 Linux)
cross build --release --target aarch64-unknown-linux-gnu
```

Cross-compilation uses `Cross.toml` to passthrough the env vars into the container.

## Architecture

This is a single Cargo crate that produces both a **library** (`baidu_pcs_rs_sdk`) and a **CLI binary** (`baidu-pcs-cli-rs`).

### Library (SDK) — `src/lib.rs` + `src/baidu_pcs_sdk/`

- `lib.rs` — Public types: `BaiduPcsApp`, `PcsAccessToken`, `AppError`, API response structs (`PcsFileItem`, `PcsDiskQuota`, etc.)
- `baidu_pcs_sdk/pcs.rs` — `BaiduPcsClient`: core API client wrapping reqwest + tokio. Handles quota, directory listing, chunked upload, download, delete. Each client owns its own `tokio::runtime::Runtime` (blocking bridge over async reqwest).
- `baidu_pcs_sdk/pcs_device_auth_impl.rs` — OAuth Device Code authorization flow (`BaiduPanDeviceAuthClient` trait + `BaiduPanClient` impl).
- `baidu_pcs_sdk/pcs_error.rs` — `AppError` with `AppErrorType` enum (`Network`/`Server`/`Client`/`Unknown`). Has `From` impls for `reqwest::Error`, `std::io::Error`, `serde_json::Error`, `PcsApiError`, and `PcsError`.

### CLI Binary — `src/main.rs` + supporting modules

- `main.rs` — Entry point. Reads compile-time app credentials, initializes logging, loads config, creates `BaiduPcsClient`, dispatches subcommands.
- `cli.rs` — clap `derive` definitions for CLI args and subcommands (`Auth`, `Upload`, `Download`, `List`, `Remove`, `Quota`).
- `config.rs` — TOML config file (`Config` struct) for persisted token, paths, and DNS settings. Default location: `~/.config/baidu-pcs-rs/config.toml`.
- `sync.rs` — Upload/download task orchestration with progress bars (`indicatif`).
- `auth.rs` — CLI-side auth flow: device code polling, token refresh, first-use prompt.

### DNS — `src/dns.rs`

Custom DNS resolution using `hickory-resolver` injected into reqwest via `Resolve` trait. Parses comma-separated IP/IP:port strings. Used by both `BaiduPcsClient` and `BaiduPanClient`.

## Key Conventions

- **Language**: All user-facing strings, comments, and doc comments are in **Chinese**. Maintain this convention.
- **Getter generation**: Structs use `#[derive(Getters)]` from the `getset` crate with `#[getset(get = "pub")]` or `#[getset(get = "pub with_prefix")]` (adds `get_` prefix). Avoid writing manual getters when `getset` suffices.
- **Serde flexibility**: API response structs use `#[serde(alias = "...")]` extensively to handle inconsistent field names from the Baidu API (e.g. `errno` vs `error_code`, `err_msg` vs `errmsg`). The `from_str_or_int` custom deserializer handles fields that may arrive as string or number.
- **Error pattern**: All SDK methods return `Result<T, AppError>`. Convert foreign errors via `From` impls in `pcs_error.rs`. Baidu API error codes are translated to Chinese descriptions in `try_translate_errno`.
- **Async bridge**: The SDK is internally async (reqwest + tokio) but exposes a **synchronous** API. Each client creates its own `tokio::runtime::Runtime` and calls `runtime.block_on()`.
- **Config file permissions**: On Linux/macOS, config files are created with mode `0o600` (user-only read/write) since they contain tokens.
