//! Request handler trait for IPC message processing.

use async_trait::async_trait;

use crate::error::Result;
use crate::messages::{Request, Response};

/// Trait for handling IPC requests from shell extensions.
///
/// Implementors process a [`Request`] and return a [`Response`].
/// The concrete implementation lives in the daemon crate (issue #30).
#[async_trait]
pub trait RequestHandler: Send + Sync {
    async fn handle(&self, request: Request) -> Result<Response>;
}
