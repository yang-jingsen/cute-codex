use std::future::Future;
use tokio_util::sync::CancellationToken;

/// Stack budget for threads that poll Codex async work.
pub const THREAD_STACK_SIZE_BYTES: usize = 16 * 1024 * 1024;

/// Returns the stack budget for threads that poll Codex async work.
///
/// [`std::thread::Builder::stack_size`] takes precedence over `RUST_MIN_STACK`.
/// Preserve the Codex 16 MiB floor while honoring a larger explicit override.
pub fn thread_stack_size_bytes() -> usize {
    thread_stack_size_bytes_from_override(std::env::var("RUST_MIN_STACK").ok().as_deref())
}

fn thread_stack_size_bytes_from_override(value: Option<&str>) -> usize {
    value
        .and_then(|value| value.parse::<usize>().ok())
        .map_or(THREAD_STACK_SIZE_BYTES, |value| {
            value.max(THREAD_STACK_SIZE_BYTES)
        })
}

#[derive(Debug, PartialEq, Eq)]
pub enum CancelErr {
    Cancelled,
}

pub trait OrCancelExt: Sized {
    type Output;

    fn or_cancel(
        self,
        token: &CancellationToken,
    ) -> impl Future<Output = Result<Self::Output, CancelErr>> + Send;
}

impl<F> OrCancelExt for F
where
    F: Future + Send,
    F::Output: Send,
{
    type Output = F::Output;

    async fn or_cancel(self, token: &CancellationToken) -> Result<Self::Output, CancelErr> {
        tokio::select! {
            _ = token.cancelled() => Err(CancelErr::Cancelled),
            res = self => Ok(res),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::time::Duration;
    use tokio::task;
    use tokio::time::sleep;

    #[test]
    fn thread_stack_size_preserves_default_without_valid_larger_override() {
        for value in [None, Some(""), Some("invalid"), Some("8388608")] {
            assert_eq!(
                THREAD_STACK_SIZE_BYTES,
                thread_stack_size_bytes_from_override(value)
            );
        }
    }

    #[test]
    fn thread_stack_size_honors_larger_override() {
        assert_eq!(
            32 * 1024 * 1024,
            thread_stack_size_bytes_from_override(Some("33554432"))
        );
    }

    #[tokio::test]
    async fn returns_ok_when_future_completes_first() {
        let token = CancellationToken::new();
        let value = async { 42 };

        let result = value.or_cancel(&token).await;

        assert_eq!(Ok(42), result);
    }

    #[tokio::test]
    async fn returns_err_when_token_cancelled_first() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let cancel_handle = task::spawn(async move {
            sleep(Duration::from_millis(10)).await;
            token_clone.cancel();
        });

        let result = async {
            sleep(Duration::from_millis(100)).await;
            7
        }
        .or_cancel(&token)
        .await;

        cancel_handle.await.expect("cancel task panicked");
        assert_eq!(Err(CancelErr::Cancelled), result);
    }

    #[tokio::test]
    async fn returns_err_when_token_already_cancelled() {
        let token = CancellationToken::new();
        token.cancel();

        let result = async {
            sleep(Duration::from_millis(50)).await;
            5
        }
        .or_cancel(&token)
        .await;

        assert_eq!(Err(CancelErr::Cancelled), result);
    }
}
