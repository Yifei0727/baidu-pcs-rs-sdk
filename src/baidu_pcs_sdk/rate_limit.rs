//! 全局速率限制器
//!
//! 提供基于令牌桶（token bucket）的速率限制，用于限制上传/下载的吞吐量。
//! 同时提供 `parse_bandwidth` 函数解析形如 `100k`、`1M`、`100M`、`1KB`、`1KiB`
//! 的带宽字符串为字节/秒数值。
//!
//! 约定（与 curl/wget 等文件传输工具一致）：
//! - `k`/`K`/`KB`/`KiB` -> 1024 倍
//! - `m`/`M`/`MB`/`MiB` -> 1024^2 倍
//! - `g`/`G`/`GB`/`GiB` -> 1024^3 倍
//! - 无单位或 `B` -> 1 倍（字节/秒）

use bytes::Bytes;
use futures::stream::Stream;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{ready, Context, Poll};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, ReadBuf};

/// 令牌桶速率限制器
///
/// 桶容量为 1 秒钟的流量（即允许 1 秒的突发），按 `rate` 字节/秒的速率补充令牌。
/// 线程安全（内部使用 `Mutex`），可被多个并发任务共享。
pub struct RateLimiter {
    /// 每秒允许的字节数，0 表示不限速
    rate: u64,
    state: Mutex<State>,
}

struct State {
    /// 当前可用令牌数（字节数），可以为负数（表示已经透支）
    avail: f64,
    /// 上次令牌补充时间
    last: Instant,
}

impl RateLimiter {
    /// 创建一个每秒 `rate` 字节的速率限制器
    pub fn new(rate: u64) -> Self {
        Self {
            rate,
            state: Mutex::new(State {
                // 初始令牌 = 1 秒突发量
                avail: rate as f64,
                last: Instant::now(),
            }),
        }
    }

    /// 返回限速速率（字节/秒），0 表示不限速
    pub fn rate(&self) -> u64 {
        self.rate
    }

    /// 消耗 `n` 字节的令牌
    ///
    /// 返回 `Some(duration)` 表示需要睡眠 `duration` 后才能继续；返回 `None` 表示令牌充足。
    /// 无论返回什么，令牌都会被扣除（可能为负数，下次调用时会按时间补充）。
    pub fn consume(&self, n: u64) -> Option<Duration> {
        if self.rate == 0 {
            return None;
        }
        let mut s = self.state.lock().unwrap();
        let now = Instant::now();
        let elapsed = now.duration_since(s.last).as_secs_f64();
        s.last = now;
        // 补充令牌，但不超过 `rate`（突发上限 = 1 秒流量）
        s.avail = (s.avail + elapsed * self.rate as f64).min(self.rate as f64);
        s.avail -= n as f64;
        if s.avail < 0.0 {
            let deficit = -s.avail;
            Some(Duration::from_secs_f64(deficit / self.rate as f64))
        } else {
            None
        }
    }

    /// 异步获取 `n` 字节的令牌，必要时睡眠等待
    pub async fn acquire(&self, n: u64) {
        if let Some(dur) = self.consume(n) {
            if !dur.is_zero() {
                tokio::time::sleep(dur).await;
            }
        }
    }
}

/// 解析带宽字符串为字节/秒
///
/// 支持格式（大小写不敏感）：
/// - `100` / `100B` -> 100 字节/秒
/// - `100k` / `100K` / `100KB` / `100KiB` -> 100 * 1024 字节/秒
/// - `1M` / `1MB` / `1MiB` -> 1 * 1024 * 1024 字节/秒
/// - `2G` / `2GB` / `2GiB` -> 2 * 1024^3 字节/秒
pub fn parse_bandwidth(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("带宽字符串为空".to_string());
    }

    // 找到数字部分与单位部分的分界
    let mut split_idx = s.len();
    for (i, c) in s.char_indices() {
        if !(c.is_ascii_digit() || c == '.') {
            split_idx = i;
            break;
        }
    }

    let (num_str, unit_str) = s.split_at(split_idx);
    let num: f64 = num_str
        .parse()
        .map_err(|_| format!("无效的数字: {}", num_str))?;

    let unit_lower = unit_str.trim().to_lowercase();
    let mult: u64 = match unit_lower.as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        "t" | "tb" | "tib" => 1024u64 * 1024 * 1024 * 1024,
        other => return Err(format!("未知的带宽单位: {}", other)),
    };

    let bytes = (num * mult as f64) as u64;
    if bytes == 0 {
        return Err(format!("带宽为 0: {}", s));
    }
    Ok(bytes)
}

/// 读取缓冲区大小
const STREAM_CHUNK_SIZE: usize = 8 * 1024;

/// 带速率限制的字节流
///
/// 包装一个 `AsyncRead`，按 `RateLimiter` 限制每次读取的吞吐量。
/// 实现 `Stream<Item = io::Result<Bytes>>`，可直接用于 `reqwest::Body::wrap_stream`。
pub struct RateLimitedStream<R> {
    reader: R,
    limiter: Arc<RateLimiter>,
    buf: Vec<u8>,
    /// 读取完成但需等待令牌恢复后再产出的数据块
    pending: Option<Bytes>,
    /// 令牌不足时的睡眠 future（pin 在堆上以支持 poll）
    sleep: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl<R: AsyncRead + Unpin> RateLimitedStream<R> {
    pub fn new(reader: R, limiter: Arc<RateLimiter>) -> Self {
        Self {
            reader,
            limiter,
            buf: vec![0u8; STREAM_CHUNK_SIZE],
            pending: None,
            sleep: None,
        }
    }
}

impl<R: AsyncRead + Unpin> Stream for RateLimitedStream<R> {
    type Item = io::Result<Bytes>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // RateLimitedStream 是 Unpin（所有字段均为 Unpin），可以安全获取 &mut
        let this = self.get_mut();
        loop {
            // 如果正在睡眠，先等待睡眠结束
            if let Some(sleep) = this.sleep.as_mut() {
                if sleep.as_mut().poll(cx).is_pending() {
                    return Poll::Pending;
                }
                this.sleep = None;
                // 睡眠结束，把暂存的数据块产出
                if let Some(chunk) = this.pending.take() {
                    return Poll::Ready(Some(Ok(chunk)));
                }
            }

            // 从底层 reader 读取一个 chunk
            let mut read_buf = ReadBuf::new(&mut this.buf);
            match ready!(Pin::new(&mut this.reader).poll_read(cx, &mut read_buf)) {
                Ok(()) => {
                    let n = read_buf.filled().len();
                    if n == 0 {
                        return Poll::Ready(None);
                    }
                    let chunk = Bytes::copy_from_slice(&this.buf[..n]);
                    // 消耗令牌；若不足则睡眠后再产出
                    if let Some(dur) = this.limiter.consume(n as u64) {
                        if !dur.is_zero() {
                            this.pending = Some(chunk);
                            let mut sleep = Box::pin(tokio::time::sleep(dur));
                            // 立即 poll 一次以注册 waker
                            if sleep.as_mut().poll(cx).is_pending() {
                                this.sleep = Some(sleep);
                                return Poll::Pending;
                            }
                            // 睡眠已立即完成（罕见），直接产出
                            return Poll::Ready(Some(Ok(this.pending.take().unwrap())));
                        }
                    }
                    return Poll::Ready(Some(Ok(chunk)));
                }
                Err(e) => return Poll::Ready(Some(Err(e))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bandwidth() {
        assert_eq!(parse_bandwidth("100").unwrap(), 100);
        assert_eq!(parse_bandwidth("100B").unwrap(), 100);
        assert_eq!(parse_bandwidth("100b").unwrap(), 100);
        assert_eq!(parse_bandwidth("100k").unwrap(), 100 * 1024);
        assert_eq!(parse_bandwidth("100K").unwrap(), 100 * 1024);
        assert_eq!(parse_bandwidth("100kb").unwrap(), 100 * 1024);
        assert_eq!(parse_bandwidth("100KB").unwrap(), 100 * 1024);
        assert_eq!(parse_bandwidth("100kib").unwrap(), 100 * 1024);
        assert_eq!(parse_bandwidth("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_bandwidth("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_bandwidth("1MiB").unwrap(), 1024 * 1024);
        assert_eq!(parse_bandwidth("100M").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_bandwidth("2G").unwrap(), 2 * 1024 * 1024 * 1024);
        // 支持小数
        assert_eq!(parse_bandwidth("1.5k").unwrap(), (1.5 * 1024.0) as u64);
        assert_eq!(
            parse_bandwidth("0.5M").unwrap(),
            (0.5 * 1024.0 * 1024.0) as u64
        );
        // 带空格
        assert_eq!(parse_bandwidth(" 100k ").unwrap(), 100 * 1024);
        assert_eq!(parse_bandwidth("100 k").unwrap(), 100 * 1024);
    }

    #[test]
    fn test_parse_bandwidth_errors() {
        assert!(parse_bandwidth("").is_err());
        assert!(parse_bandwidth("abc").is_err());
        assert!(parse_bandwidth("100x").is_err());
        assert!(parse_bandwidth("0").is_err());
        assert!(parse_bandwidth("0k").is_err());
    }

    #[test]
    fn test_rate_limiter_no_limit() {
        let limiter = RateLimiter::new(0);
        // rate=0 表示不限速
        assert!(limiter.consume(1000).is_none());
        assert!(limiter.consume(1_000_000).is_none());
    }

    #[test]
    fn test_rate_limiter_burst() {
        let limiter = RateLimiter::new(1024); // 1KB/s, 突发 1KB
        // 初始令牌 = 1024，消耗 1024 应该不需要睡眠
        assert!(limiter.consume(1024).is_none());
        // 再消耗 1 字节应该需要睡眠
        let sleep = limiter.consume(1);
        assert!(sleep.is_some());
        let sleep = sleep.unwrap();
        assert!(sleep.as_secs_f64() > 0.0);
    }
}
