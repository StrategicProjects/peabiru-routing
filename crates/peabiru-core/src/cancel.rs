//! Cancelling a long computation from another thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A flag that stops a running computation. Every routing parameter
/// struct carries one ([`RouteParams::cancel`](crate::street::RouteParams),
/// [`TransitParams::cancel`](crate::transit::matrix::TransitParams)); keep
/// a clone, hand the parameters to the call, and call
/// [`cancel`](CancelToken::cancel) from any thread. The workers notice
/// between origins (between batches of eight in a street matrix computed
/// by contraction hierarchy, and between the rounds of the contraction)
/// and, from a transit origin, at every departure minute and Monte Carlo
/// draw and between destinations; the call then returns
/// [`RouteError::Cancelled`] instead of a partial result. The check is one
/// relaxed atomic load.
///
/// Clones share the flag. Once cancelled, a token stays cancelled: every
/// later call made with it returns `Cancelled` without routing a single
/// origin, so make a new one (the default) for the next call. The setup
/// before the first origin (validating the parameters, linking the
/// destinations to the streets) is not interrupted.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the computations that hold this token to stop.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Whether [`cancel`](CancelToken::cancel) was called on this token or
    /// a clone of it.
    #[inline]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// `Err(Cancelled)` once cancelled.
    pub(crate) fn check(&self) -> Result<(), RouteError> {
        if self.is_cancelled() {
            Err(RouteError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Why a routing computation returned no result.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RouteError {
    /// The parameters or the inputs were rejected; the message says why.
    Invalid(String),
    /// The call's [`CancelToken`] was cancelled before it finished.
    Cancelled,
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::Invalid(msg) => f.write_str(msg),
            RouteError::Cancelled => f.write_str("the computation was cancelled"),
        }
    }
}

impl std::error::Error for RouteError {}

impl From<String> for RouteError {
    fn from(msg: String) -> Self {
        RouteError::Invalid(msg)
    }
}

impl From<&str> for RouteError {
    fn from(msg: &str) -> Self {
        RouteError::Invalid(msg.to_string())
    }
}

impl From<crate::street::router::ParamError> for RouteError {
    fn from(e: crate::street::router::ParamError) -> Self {
        RouteError::Invalid(e.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_flag() {
        let a = CancelToken::new();
        let b = a.clone();
        assert!(!b.is_cancelled());
        assert_eq!(b.check(), Ok(()));
        a.cancel();
        assert!(b.is_cancelled());
        assert_eq!(b.check(), Err(RouteError::Cancelled));
        assert!(!CancelToken::default().is_cancelled());
    }
}
