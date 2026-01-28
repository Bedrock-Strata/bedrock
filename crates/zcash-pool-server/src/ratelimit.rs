//! Rate limiting for share submissions
//!
//! Prevents clients from overwhelming the server with share submissions.

use std::time::{Duration, Instant};

/// Token bucket rate limiter
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum tokens (burst capacity)
    capacity: u32,
    /// Current token count
    tokens: f64,
    /// Tokens added per second
    refill_rate: f64,
    /// Last time tokens were refilled
    last_refill: Instant,
}

/// Result of a rate limit check
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitResult {
    /// Request is allowed
    Allowed,
    /// Request is rate limited
    Limited,
}

impl RateLimitResult {
    /// Returns true if the request is allowed
    pub fn is_allowed(&self) -> bool {
        matches!(self, RateLimitResult::Allowed)
    }
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    /// * `capacity` - Maximum burst size (number of requests that can be made at once)
    /// * `refill_rate` - Number of tokens added per second
    pub fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Create a rate limiter for share submissions
    ///
    /// Default: 100 shares burst, 20 shares/second sustained
    pub fn for_shares() -> Self {
        Self::new(100, 20.0)
    }

    /// Check if a request should be allowed and consume a token if so
    pub fn check(&mut self) -> RateLimitResult {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            RateLimitResult::Allowed
        } else {
            RateLimitResult::Limited
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        self.last_refill = now;

        let new_tokens = elapsed.as_secs_f64() * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity as f64);
    }

    /// Get the current token count (for monitoring)
    pub fn available_tokens(&self) -> u32 {
        self.tokens as u32
    }

    /// Get time until next token is available
    pub fn time_until_available(&self) -> Duration {
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else {
            let needed = 1.0 - self.tokens;
            Duration::from_secs_f64(needed / self.refill_rate)
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::for_shares()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_basic_rate_limiting() {
        let mut limiter = RateLimiter::new(3, 1.0);

        // First 3 requests should be allowed (burst)
        assert!(limiter.check().is_allowed());
        assert!(limiter.check().is_allowed());
        assert!(limiter.check().is_allowed());

        // 4th request should be limited
        assert!(!limiter.check().is_allowed());
    }

    #[test]
    fn test_refill() {
        let mut limiter = RateLimiter::new(2, 10.0); // 10 tokens/sec

        // Exhaust tokens
        assert!(limiter.check().is_allowed());
        assert!(limiter.check().is_allowed());
        assert!(!limiter.check().is_allowed());

        // Wait for refill (100ms = 1 token at 10/sec)
        thread::sleep(Duration::from_millis(150));

        // Should have 1 token now
        assert!(limiter.check().is_allowed());
        assert!(!limiter.check().is_allowed());
    }

    #[test]
    fn test_for_shares_defaults() {
        let limiter = RateLimiter::for_shares();
        assert_eq!(limiter.capacity, 100);
        assert_eq!(limiter.refill_rate, 20.0);
    }

    #[test]
    fn test_time_until_available() {
        let mut limiter = RateLimiter::new(1, 10.0);

        // With token available
        assert_eq!(limiter.time_until_available(), Duration::ZERO);

        // After consuming
        limiter.check();
        let wait_time = limiter.time_until_available();
        assert!(wait_time > Duration::ZERO);
        assert!(wait_time <= Duration::from_millis(100));
    }
}
