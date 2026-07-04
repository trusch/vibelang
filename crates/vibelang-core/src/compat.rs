//! Platform compatibility layer for WASM and native targets.
//!
//! This module provides abstractions over platform-specific async primitives,
//! allowing vibelang-core to compile for both native and WASM targets.

// ============================================================================
// Re-exports for sync primitives
// ============================================================================

pub use std::sync::Arc;

// ============================================================================
// RwLock abstractions
// ============================================================================

// On native, use tokio's async RwLock which can be held across await points
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::sync::RwLock;

// On WASM, we need a wrapper that provides async-like interface for std RwLock
#[cfg(target_arch = "wasm32")]
mod wasm_rwlock {
    use std::ops::{Deref, DerefMut};

    /// A RwLock wrapper that provides an async-like interface on WASM.
    /// Since WASM is single-threaded, this is just a thin wrapper around std::sync::RwLock.
    pub struct RwLock<T>(std::sync::RwLock<T>);

    impl<T> RwLock<T> {
        pub fn new(value: T) -> Self {
            Self(std::sync::RwLock::new(value))
        }

        /// Acquire a read lock. Returns a future that resolves immediately on WASM.
        pub async fn read(&self) -> RwLockReadGuard<'_, T> {
            RwLockReadGuard(self.0.read().expect("RwLock poisoned"))
        }

        /// Acquire a write lock. Returns a future that resolves immediately on WASM.
        pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
            RwLockWriteGuard(self.0.write().expect("RwLock poisoned"))
        }
    }

    pub struct RwLockReadGuard<'a, T>(std::sync::RwLockReadGuard<'a, T>);

    impl<T> Deref for RwLockReadGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    pub struct RwLockWriteGuard<'a, T>(std::sync::RwLockWriteGuard<'a, T>);

    impl<T> Deref for RwLockWriteGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> DerefMut for RwLockWriteGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_rwlock::RwLock;

// ============================================================================
// Time abstractions
// ============================================================================

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Duration, Instant};

#[cfg(target_arch = "wasm32")]
pub use web_time::{Duration, Instant};

// ============================================================================
// Channel abstractions
// ============================================================================

/// MPSC channel sender.
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::sync::mpsc::Sender;

#[cfg(target_arch = "wasm32")]
pub use futures::channel::mpsc::Sender;

/// MPSC channel receiver.
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::sync::mpsc::Receiver;

#[cfg(target_arch = "wasm32")]
pub use futures::channel::mpsc::Receiver;

/// Oneshot channel sender.
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::sync::oneshot::Sender as OneshotSender;

#[cfg(target_arch = "wasm32")]
pub use futures::channel::oneshot::Sender as OneshotSender;

/// Oneshot channel receiver.
#[cfg(not(target_arch = "wasm32"))]
pub use tokio::sync::oneshot::Receiver as OneshotReceiver;

#[cfg(target_arch = "wasm32")]
pub use futures::channel::oneshot::Receiver as OneshotReceiver;

/// Create a bounded MPSC channel.
#[cfg(not(target_arch = "wasm32"))]
pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    tokio::sync::mpsc::channel(buffer)
}

#[cfg(target_arch = "wasm32")]
pub fn channel<T>(buffer: usize) -> (Sender<T>, Receiver<T>) {
    futures::channel::mpsc::channel(buffer)
}

/// Create a oneshot channel.
#[cfg(not(target_arch = "wasm32"))]
pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    tokio::sync::oneshot::channel()
}

#[cfg(target_arch = "wasm32")]
pub fn oneshot<T>() -> (OneshotSender<T>, OneshotReceiver<T>) {
    futures::channel::oneshot::channel()
}

// ============================================================================
// Async utilities
// ============================================================================

/// Sleep for the given duration.
#[cfg(not(target_arch = "wasm32"))]
pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

#[cfg(target_arch = "wasm32")]
pub async fn sleep(duration: Duration) {
    gloo_timers::future::TimeoutFuture::new(duration.as_millis() as u32).await;
}

/// Result type for timeout operations.
pub type TimeoutResult<T> = std::result::Result<T, TimeoutError>;

/// Error returned when a timeout expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutError;

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation timed out")
    }
}

impl std::error::Error for TimeoutError {}

/// Run a future with a timeout.
#[cfg(not(target_arch = "wasm32"))]
pub async fn timeout<F, T>(duration: Duration, future: F) -> TimeoutResult<T>
where
    F: std::future::Future<Output = T>,
{
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| TimeoutError)
}

#[cfg(target_arch = "wasm32")]
pub async fn timeout<F, T>(duration: Duration, future: F) -> TimeoutResult<T>
where
    F: std::future::Future<Output = T>,
{
    use futures::future::{select, Either};
    use gloo_timers::future::TimeoutFuture;

    let timeout_fut = TimeoutFuture::new(duration.as_millis() as u32);
    match select(std::pin::pin!(future), timeout_fut).await {
        Either::Left((result, _)) => Ok(result),
        Either::Right((_, _)) => Err(TimeoutError),
    }
}

// ============================================================================
// Receiver extension trait for try_recv
// ============================================================================

/// Extension trait for receivers to provide try_recv functionality.
#[cfg(not(target_arch = "wasm32"))]
pub trait ReceiverExt<T> {
    /// Try to receive a message without blocking.
    fn try_recv_compat(&mut self) -> Option<T>;
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> ReceiverExt<T> for Receiver<T> {
    fn try_recv_compat(&mut self) -> Option<T> {
        self.try_recv().ok()
    }
}

#[cfg(target_arch = "wasm32")]
pub trait ReceiverExt<T> {
    /// Try to receive a message without blocking.
    fn try_recv_compat(&mut self) -> Option<T>;
}

#[cfg(target_arch = "wasm32")]
impl<T> ReceiverExt<T> for Receiver<T> {
    fn try_recv_compat(&mut self) -> Option<T> {
        // Receiver has inherent try_next() -> Result<Option<T>, TryRecvError>
        match self.try_next() {
            Ok(Some(item)) => Some(item),
            _ => None,
        }
    }
}

// ============================================================================
// Sender extension trait for send
// ============================================================================

/// Extension trait for senders to provide async send functionality.
pub trait SenderExt<T> {
    /// Send a message asynchronously.
    fn send_async(
        &self,
        item: T,
    ) -> impl std::future::Future<Output = std::result::Result<(), SendError<T>>> + Send;
}

/// Error returned when send fails.
#[derive(Debug)]
pub struct SendError<T>(pub T);

impl<T> std::fmt::Display for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "channel closed")
    }
}

impl<T: std::fmt::Debug> std::error::Error for SendError<T> {}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> SenderExt<T> for Sender<T> {
    async fn send_async(&self, item: T) -> std::result::Result<(), SendError<T>> {
        self.send(item).await.map_err(|e| SendError(e.0))
    }
}

#[cfg(target_arch = "wasm32")]
impl<T: Send> SenderExt<T> for Sender<T> {
    async fn send_async(&self, item: T) -> std::result::Result<(), SendError<T>> {
        use std::future::poll_fn;

        // Try to send synchronously first
        let mut sender = self.clone();
        match sender.try_send(item) {
            Ok(()) => Ok(()),
            Err(e) if e.is_disconnected() => {
                // Channel closed - return the item in error
                Err(SendError(e.into_inner()))
            }
            Err(e) if e.is_full() => {
                // Channel full - wait for capacity and retry
                let item = e.into_inner();
                // Wait for the channel to be ready using poll_ready
                let ready_result = poll_fn(|cx| sender.poll_ready(cx)).await;
                if ready_result.is_err() {
                    // Channel disconnected while waiting
                    return Err(SendError(item));
                }
                // Now try_send should succeed (channel ready and not disconnected)
                sender.try_send(item).map_err(|e| SendError(e.into_inner()))
            }
            Err(e) => Err(SendError(e.into_inner())),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration() {
        let d = Duration::from_millis(100);
        assert_eq!(d.as_millis(), 100);
    }

    #[test]
    fn test_channel_creation() {
        let (_tx, _rx) = channel::<i32>(10);
    }

    #[test]
    fn test_oneshot_creation() {
        let (_tx, _rx) = oneshot::<i32>();
    }
}
