//! Token-bucket rate limits (`docs/wallet.md` §6.3).

use std::time::Duration;

use thiserror::Error;

use crate::types::TimestampMs;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenBucket {
    pub capacity: u32,
    pub window_ms: u64,
    pub tokens: u32,
    pub window_start_ms: TimestampMs,
}

impl TokenBucket {
    pub fn new(capacity: u32, window: Duration, now_ms: TimestampMs) -> Self {
        Self {
            capacity,
            window_ms: window.as_millis() as u64,
            tokens: capacity,
            window_start_ms: now_ms,
        }
    }

    fn refill(&mut self, now_ms: TimestampMs) {
        if self.window_ms == 0 {
            return;
        }
        let elapsed = now_ms.saturating_sub(self.window_start_ms);
        if elapsed >= self.window_ms {
            let windows = elapsed / self.window_ms;
            self.tokens = self.capacity;
            self.window_start_ms = self.window_start_ms.saturating_add(windows * self.window_ms);
        }
    }

    /// Consume one token if available (atomic with caller's reserve).
    pub fn try_consume(&mut self, now_ms: TimestampMs) -> Result<(), RateLimitError> {
        self.refill(now_ms);
        if self.tokens == 0 {
            return Err(RateLimitError::Exceeded);
        }
        self.tokens -= 1;
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RateLimitError {
    #[error("rate limit exceeded")]
    Exceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_resets_after_window() {
        let mut b = TokenBucket::new(2, Duration::from_millis(1000), 0);
        b.try_consume(0).unwrap();
        b.try_consume(0).unwrap();
        assert_eq!(b.try_consume(0), Err(RateLimitError::Exceeded));
        b.try_consume(1000).unwrap(); // new window
    }
}
