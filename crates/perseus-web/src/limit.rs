use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

const MAX_TRACKED_CLIENTS: usize = 10_000;

pub struct RateLimiter {
    limit: u32,
    window: Duration,
    clients: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            clients: Mutex::new(HashMap::new()),
        }
    }

    pub fn check(&self, client: &str, now: Instant) -> Result<(), u64> {
        let mut clients = self.clients.lock().unwrap_or_else(PoisonError::into_inner);
        if clients.len() >= MAX_TRACKED_CLIENTS && !clients.contains_key(client) {
            clients.retain(|_, (start, _)| now.duration_since(*start) < self.window);
        }
        let (start, count) = clients.entry(client.to_owned()).or_insert((now, 0));
        if now.duration_since(*start) >= self.window {
            *start = now;
            *count = 0;
        }
        if *count >= self.limit {
            let left = self.window.saturating_sub(now.duration_since(*start));
            return Err(left.as_secs().max(1));
        }
        *count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_limit_and_resets_with_the_window() {
        let limiter = RateLimiter::new(2, Duration::from_secs(60));
        let start = Instant::now();
        assert!(limiter.check("a", start).is_ok());
        assert!(limiter.check("a", start).is_ok());
        assert_eq!(limiter.check("a", start + Duration::from_secs(20)), Err(40));
        assert!(limiter.check("b", start).is_ok(), "limite por cliente");
        assert!(limiter.check("a", start + Duration::from_secs(60)).is_ok());
    }

    #[test]
    fn prunes_expired_clients_when_full() {
        let limiter = RateLimiter::new(1, Duration::from_secs(1));
        let start = Instant::now();
        for index in 0..MAX_TRACKED_CLIENTS {
            assert!(limiter.check(&index.to_string(), start).is_ok());
        }
        assert!(
            limiter
                .check("novo", start + Duration::from_secs(2))
                .is_ok()
        );
        let tracked = limiter
            .clients
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len();
        assert_eq!(tracked, 1);
    }
}
