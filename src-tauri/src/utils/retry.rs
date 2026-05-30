//! 通用重试策略工具
//!
//! 提供指数退避重试逻辑，可用于 AI 调用、网络请求、文件操作等需要自动重试的场景。
//! 支持可配置的最大重试次数、初始延迟、退避倍率、最大延迟和随机抖动。

use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;

/// 重试策略配置
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最大重试次数（不含首次尝试）
    pub max_retries: u32,
    /// 初始等待时间（毫秒）
    pub initial_delay_ms: u64,
    /// 退避倍率（每次重试 delay *= multiplier）
    pub backoff_multiplier: f64,
    /// 最大等待时间上限（毫秒）
    pub max_delay_ms: u64,
    /// 是否添加随机抖动（避免惊群效应）
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 30_000,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// 创建默认的 AI API 重试策略（指数退避，最多 2 次重试）
    pub fn ai_default() -> Self {
        Self {
            max_retries: 2,
            initial_delay_ms: 2000,
            backoff_multiplier: 2.0,
            max_delay_ms: 60_000,
            jitter: true,
        }
    }

    /// 创建文件 I/O 重试策略（短间隔，快速重试）
    pub fn io_default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 500,
            backoff_multiplier: 1.5,
            max_delay_ms: 5_000,
            jitter: true,
        }
    }

    /// 计算第 `attempt` 次重试（0-based）的等待时间
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let raw = std::cmp::min(
            self.initial_delay_ms * (self.backoff_multiplier.powi(attempt as i32) as u64),
            self.max_delay_ms,
        );
        let ms = if self.jitter {
            let mut rng = rand::thread_rng();
            if raw > 0 {
                rng.gen_range(0..=raw)
            } else {
                0
            }
        } else {
            raw
        };
        Duration::from_millis(ms)
    }

    /// 执行带重试的异步操作。
    ///
    /// - `operation`: 异步闭包，返回 `Result<T, E>`。
    /// - `is_retryable`: 判断错误是否可重试的闭包。
    /// - `label`: 用于日志记录的描述文本。
    pub async fn run<F, Fut, T, E, P>(
        &self,
        mut operation: F,
        is_retryable: P,
        label: &str,
    ) -> Result<T, E>
    where
        F: FnMut(u32) -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        P: Fn(&E) -> bool,
        E: std::fmt::Display,
    {
        let mut last_err: Option<E> = None;

        for attempt in 0..=self.max_retries {
            match operation(attempt).await {
                Ok(v) => {
                    if attempt > 0 {
                        tracing::info!(
                            "[Retry] '{}' 第{}次重试成功",
                            label,
                            attempt
                        );
                    }
                    return Ok(v);
                }
                Err(e) => {
                    if attempt == self.max_retries || !is_retryable(&e) {
                        tracing::error!(
                            "[Retry] '{}' 最终失败 (第{}次): {}",
                            label,
                            attempt,
                            e
                        );
                        return Err(e);
                    }
                    last_err = Some(e);
                    let delay = self.delay_for(attempt);
                    tracing::warn!(
                        "[Retry] '{}' 第{}次失败，{}ms后重试...",
                        label,
                        attempt + 1,
                        delay.as_millis()
                    );
                    sleep(delay).await;
                }
            }
        }

        // 理论上不会到达这里，但保留兜底
        Err(last_err.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_no_jitter() {
        let policy = RetryPolicy {
            max_retries: 2,
            initial_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_delay_ms: 10000,
            jitter: false,
        };
        assert_eq!(policy.delay_for(0).as_millis(), 1000);
        assert_eq!(policy.delay_for(1).as_millis(), 2000);
        assert_eq!(policy.delay_for(2).as_millis(), 4000);
    }

    #[test]
    fn test_delay_respects_max() {
        let policy = RetryPolicy {
            max_retries: 5,
            initial_delay_ms: 5000,
            backoff_multiplier: 3.0,
            max_delay_ms: 30000,
            jitter: false,
        };
        // 5000*3^2 = 45000 > 30000, should cap at 30000
        assert_eq!(policy.delay_for(2).as_millis(), 30000);
    }

    #[test]
    fn test_defaults() {
        let p = RetryPolicy::ai_default();
        assert_eq!(p.max_retries, 2);
        assert!(p.jitter);

        let p = RetryPolicy::io_default();
        assert_eq!(p.max_retries, 3);
        assert!(p.jitter);
    }
}
