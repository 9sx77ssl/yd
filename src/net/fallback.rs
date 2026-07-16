use color_eyre::eyre::Result;
use futures::future::{select, Either};
use std::future::Future;

/// Resolves `primary`, falling back to `fallback` only if `primary` fails.
///
/// Both futures are polled concurrently (so the slow provider does not gate the
/// fast one) and the primary's result is preferred whenever it succeeds. This
/// is the shared resilient-query primitive used by price feeds and any future
/// data source with redundant providers.
pub async fn with_fallback<T, F1, F2>(primary: F1, fallback: F2) -> Result<T>
where
    F1: Future<Output = Result<T>>,
    F2: Future<Output = Result<T>>,
{
    let primary = Box::pin(primary);
    let fallback = Box::pin(fallback);

    // Race them: whichever finishes first is observed. If the primary finishes
    // first with `Ok`, return it; otherwise wait for the fallback.
    match select(primary, fallback).await {
        Either::Left((Ok(value), _fallback_fut)) => Ok(value),
        Either::Left((Err(primary_error), fallback_fut)) => {
            tracing::debug!(%primary_error, "primary provider failed, trying fallback");
            fallback_fut.await
        }
        Either::Right((Ok(value), primary_fut)) => {
            // Fallback resolved first and succeeded; if the primary had failed
            // we would already have consumed it, so prefer the primary only if
            // it also produced a value. Otherwise keep the fallback result.
            match primary_fut.await {
                Ok(primary_value) => Ok(primary_value),
                Err(primary_error) => {
                    tracing::debug!(%primary_error, "primary provider failed after fallback");
                    Ok(value)
                }
            }
        }
        Either::Right((Err(fallback_error), primary_fut)) => match primary_fut.await {
            Ok(value) => Ok(value),
            Err(primary_error) => {
                tracing::debug!(%primary_error, %fallback_error, "primary and fallback providers unavailable");
                Err(fallback_error)
            }
        },
    }
}

/// Like [`with_fallback`] but tolerates total failure by returning `None`.
///
/// Use this when a missing value must not abort the caller (for example a USD
/// quote that is nice-to-have next to a balance).
pub async fn with_fallback_or_none<T, F1, F2>(primary: F1, fallback: F2) -> Option<T>
where
    F1: Future<Output = Result<T>>,
    F2: Future<Output = Result<T>>,
{
    with_fallback(primary, fallback).await.ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    async fn ok<T: Send + 'static>(value: T) -> Result<T> {
        Ok(value)
    }
    async fn err<T>(msg: &'static str) -> Result<T> {
        Err(color_eyre::eyre::eyre!("{msg}"))
    }

    #[tokio::test]
    async fn prefers_primary_when_it_succeeds() {
        let result: Result<i32> = with_fallback(ok(1), ok(2)).await;
        assert_eq!(result.unwrap(), 1);
    }

    #[tokio::test]
    async fn falls_back_when_primary_fails() {
        let result: Result<i32> = with_fallback(err::<i32>("primary down"), ok(7)).await;
        assert_eq!(result.unwrap(), 7);
    }

    #[tokio::test]
    async fn returns_none_when_both_fail() {
        let result =
            with_fallback_or_none(err::<i32>("primary down"), err::<i32>("fallback down")).await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn handles_slow_primary_with_fast_fallback() {
        // Fallback resolves first with Ok; primary is still pending and later Ok.
        // The helper must still prefer the primary's value.
        let order = Arc::new(Mutex::new(Vec::<&'static str>::new()));
        let order_primary = order.clone();
        let order_fallback = order.clone();

        let primary = async move {
            tokio::task::yield_now().await;
            tokio::task::yield_now().await;
            order_primary.lock().await.push("primary");
            Ok::<i32, color_eyre::Report>(10)
        };
        let fallback = async move {
            order_fallback.lock().await.push("fallback");
            Ok::<i32, color_eyre::Report>(20)
        };

        let result = with_fallback(primary, fallback).await.unwrap();
        // Primary is preferred when it ultimately succeeds.
        assert_eq!(result, 10);
    }
}
