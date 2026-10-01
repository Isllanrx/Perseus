use std::future::Future;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::shared::error::{Error, Result};

pub fn backoff_delay(attempt: u32, base: Duration, cap: Duration) -> Duration {
    let exponent = attempt.saturating_sub(1).min(20);
    let ceiling = base.saturating_mul(1 << exponent).min(cap);
    let half = ceiling / 2;
    half + half.mul_f64(fastrand::f64())
}

pub async fn sleep_cancellable(delay: Duration, cancel: &CancellationToken) -> Result<()> {
    tokio::select! {
        () = cancel.cancelled() => Err(Error::Cancelled),
        () = tokio::time::sleep(delay) => Ok(()),
    }
}

pub struct RetryPolicy {
    pub attempts: u32,
    pub base: Duration,
    pub cap: Duration,
}

pub async fn retry<T, F, Fut>(
    policy: &RetryPolicy,
    cancel: &CancellationToken,
    mut on_retry: impl FnMut(u32, &Error, Duration),
    mut op: F,
) -> Result<T>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let attempts = policy.attempts.max(1);
    let mut attempt = 1;
    loop {
        match op(attempt).await {
            Ok(value) => return Ok(value),
            Err(err) if attempt < attempts && err.is_retryable() && !cancel.is_cancelled() => {
                let delay = backoff_delay(attempt, policy.base, policy.cap);
                on_retry(attempt, &err, delay);
                sleep_cancellable(delay, cancel).await?;
                attempt += 1;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    #[test]
    fn backoff_stays_within_equal_jitter_bounds() {
        let base = Duration::from_secs(1);
        let cap = Duration::from_secs(30);
        for attempt in 1..=10 {
            let ceiling = base.saturating_mul(1 << (attempt - 1)).min(cap);
            let delay = backoff_delay(attempt, base, cap);
            assert!(
                delay >= ceiling / 2 && delay <= ceiling,
                "attempt {attempt}: {delay:?}"
            );
        }
    }

    fn policy(attempts: u32) -> RetryPolicy {
        RetryPolicy {
            attempts,
            base: Duration::from_millis(1),
            cap: Duration::from_millis(2),
        }
    }

    #[tokio::test]
    async fn retries_transient_errors_until_success() {
        let calls = AtomicU32::new(0);
        let retries = AtomicU32::new(0);
        let result = retry(
            &policy(3),
            &CancellationToken::new(),
            |_, _, _| {
                retries.fetch_add(1, Ordering::SeqCst);
            },
            |_| async {
                if calls.fetch_add(1, Ordering::SeqCst) < 2 {
                    Err(Error::Transfer("x".into()))
                } else {
                    Ok(7)
                }
            },
        )
        .await;
        assert_eq!(result.expect("sucesso na terceira"), 7);
        assert_eq!(retries.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn does_not_retry_permanent_errors() {
        let calls = AtomicU32::new(0);
        let result: Result<()> = retry(
            &policy(5),
            &CancellationToken::new(),
            |_, _, _| {},
            |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(Error::NotFound("x".into()))
            },
        )
        .await;
        assert!(matches!(result, Err(Error::NotFound(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancellation_interrupts_the_wait() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let slow = RetryPolicy {
            attempts: 3,
            base: Duration::from_secs(60),
            cap: Duration::from_secs(60),
        };
        let result: Result<()> = retry(
            &slow,
            &cancel,
            |_, _, _| {},
            |_| async { Err(Error::Transfer("x".into())) },
        )
        .await;
        assert!(matches!(result, Err(Error::Transfer(_))));
        assert!(matches!(
            sleep_cancellable(Duration::from_secs(60), &cancel).await,
            Err(Error::Cancelled)
        ));
    }

    #[tokio::test]
    async fn gives_up_after_the_last_attempt() {
        let calls = AtomicU32::new(0);
        let result: Result<()> = retry(
            &policy(3),
            &CancellationToken::new(),
            |_, _, _| {},
            |_| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(Error::Transfer("x".into()))
            },
        )
        .await;
        assert!(matches!(result, Err(Error::Transfer(_))));
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
}
