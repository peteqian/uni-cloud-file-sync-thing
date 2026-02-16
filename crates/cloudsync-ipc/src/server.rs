//! Unix socket IPC server.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

use crate::error::{Error, Result};
use crate::handler::RequestHandler;
use crate::messages::{Request, Response};
use crate::transport::{read_message, write_message};

/// IPC server that listens on a Unix socket for shell extension connections.
#[derive(Debug)]
pub struct IpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl IpcServer {
    /// Bind the server to the given Unix socket path.
    ///
    /// If a stale socket file exists (no process listening), it is removed.
    /// If an active server is already bound, returns an `AddrInUse` IO error.
    pub fn bind(path: impl AsRef<Path>) -> Result<Self> {
        let socket_path = path.as_ref().to_path_buf();

        if socket_path.exists() {
            // Probe whether something is actively listening
            match std::os::unix::net::UnixStream::connect(&socket_path) {
                Ok(_) => {
                    return Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::AddrInUse,
                        format!(
                            "another server is already listening on {}",
                            socket_path.display()
                        ),
                    )));
                }
                Err(_) => {
                    // Stale socket — remove it
                    debug!(path = %socket_path.display(), "removing stale socket file");
                    std::fs::remove_file(&socket_path)?;
                }
            }
        }

        let listener = UnixListener::bind(&socket_path)?;

        // Set socket permissions to owner-only (0o600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&socket_path, perms)?;
        }

        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        info!(path = %socket_path.display(), "IPC server bound");

        Ok(Self {
            listener,
            socket_path,
            shutdown_tx,
            shutdown_rx,
        })
    }

    /// Returns the path to the socket file.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Run the accept loop, spawning a task for each incoming connection.
    ///
    /// This method runs until [`shutdown`] is called or the listener errors.
    pub async fn run(&self, handler: Arc<dyn RequestHandler>) -> Result<()> {
        let mut shutdown_rx = self.shutdown_rx.clone();

        loop {
            tokio::select! {
                result = self.listener.accept() => {
                    let (stream, _addr) = result?;
                    let handler = Arc::clone(&handler);
                    let client_shutdown_rx = self.shutdown_rx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, handler, client_shutdown_rx).await {
                            warn!("client connection error: {e}");
                        }
                    });
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("IPC server shutting down");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns a handle that can signal shutdown from another task.
    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle {
            tx: self.shutdown_tx.clone(),
        }
    }

    /// Signal the server to shut down gracefully.
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            debug!(path = %self.socket_path.display(), "cleaning up socket file");
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

/// A clonable handle to signal server shutdown from another task.
#[derive(Clone, Debug)]
pub struct ShutdownHandle {
    tx: watch::Sender<bool>,
}

impl ShutdownHandle {
    /// Signal the server to shut down gracefully.
    pub fn shutdown(&self) {
        let _ = self.tx.send(true);
    }
}

/// Handle a single client connection: read requests, dispatch to handler, write responses.
async fn handle_connection(
    stream: UnixStream,
    handler: Arc<dyn RequestHandler>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    loop {
        let request: Option<Request> = tokio::select! {
            result = read_message(&mut reader) => {
                match result {
                    Ok(msg) => msg,
                    Err(Error::Json(e)) => {
                        warn!("malformed JSON from client: {e}");
                        let error_response = Response::Error {
                            message: format!("malformed JSON: {e}"),
                        };
                        write_message(&mut write_half, &error_response).await?;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    return Ok(());
                }
                continue;
            }
        };

        let Some(request) = request else {
            // Clean EOF — client disconnected
            debug!("client disconnected");
            return Ok(());
        };

        let response = match handler.handle(request).await {
            Ok(resp) => resp,
            Err(e) => {
                error!("handler error: {e}");
                Response::Error {
                    message: format!("internal error: {e}"),
                }
            }
        };

        write_message(&mut write_half, &response).await?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use tokio::io::BufReader;
    use tokio::net::UnixStream;

    /// Echo handler that wraps request info in a Response::Ok or Response::Error.
    struct EchoHandler;

    #[async_trait]
    impl RequestHandler for EchoHandler {
        async fn handle(&self, request: Request) -> Result<Response> {
            match request {
                Request::GetStatus { path } => Ok(Response::Status {
                    path,
                    state: cloudsync_core::FileState::Synced,
                    provider: Some("test".to_string()),
                    error: None,
                }),
                _ => Ok(Response::Ok),
            }
        }
    }

    /// Handler that always returns an error.
    struct ErrorHandler;

    #[async_trait]
    impl RequestHandler for ErrorHandler {
        async fn handle(&self, _request: Request) -> Result<Response> {
            Err(Error::Handler("test handler failure".to_string()))
        }
    }

    fn temp_socket_path(dir: &tempfile::TempDir, name: &str) -> PathBuf {
        dir.path().join(name)
    }

    #[tokio::test]
    async fn server_binds_and_creates_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "test.sock");

        let server = IpcServer::bind(&path).unwrap();
        assert!(path.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&path).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o600);
        }

        drop(server);
    }

    #[tokio::test]
    async fn stale_socket_cleanup_on_bind() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "stale.sock");

        // Create a stale socket file (bind then drop listener)
        let _listener = UnixListener::bind(&path).unwrap();
        drop(_listener);

        // Should succeed by cleaning up stale socket
        let server = IpcServer::bind(&path).unwrap();
        assert!(path.exists());
        drop(server);
    }

    #[tokio::test]
    async fn active_socket_returns_addr_in_use() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "active.sock");

        let _server = IpcServer::bind(&path).unwrap();

        // Second bind should fail with AddrInUse
        let result = IpcServer::bind(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            Error::Io(io_err) => {
                assert_eq!(io_err.kind(), std::io::ErrorKind::AddrInUse);
            }
            _ => panic!("expected Io(AddrInUse), got: {err:?}"),
        }
    }

    #[tokio::test]
    async fn socket_cleanup_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "drop.sock");

        let server = IpcServer::bind(&path).unwrap();
        assert!(path.exists());

        drop(server);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn single_request_response_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "roundtrip.sock");

        let server = IpcServer::bind(&path).unwrap();
        let handler: Arc<dyn RequestHandler> = Arc::new(EchoHandler);

        let server_handle = tokio::spawn({
            let handler = Arc::clone(&handler);
            async move {
                let _ = server.run(handler).await;
            }
        });

        // Give server a moment to start accepting
        tokio::task::yield_now().await;

        let stream = UnixStream::connect(&path).await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        let request = Request::GetStatus {
            path: "/test/file.txt".to_string(),
        };
        write_message(&mut write_half, &request).await.unwrap();

        let response: Option<Response> = read_message(&mut reader).await.unwrap();
        let response = response.unwrap();

        match response {
            Response::Status { path, state, .. } => {
                assert_eq!(path, "/test/file.txt");
                assert!(matches!(state, cloudsync_core::FileState::Synced));
            }
            other => panic!("expected Status response, got: {other:?}"),
        }

        // Server handle is still running — we need to stop it
        // Just drop the connection for cleanup
        drop(write_half);
        drop(reader);
        server_handle.abort();
    }

    #[tokio::test]
    async fn multiple_requests_on_same_connection() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "multi.sock");

        let server = IpcServer::bind(&path).unwrap();
        let handler: Arc<dyn RequestHandler> = Arc::new(EchoHandler);

        let server_handle = tokio::spawn({
            let handler = Arc::clone(&handler);
            async move {
                let _ = server.run(handler).await;
            }
        });

        tokio::task::yield_now().await;

        let stream = UnixStream::connect(&path).await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        // Send two requests on same connection
        for i in 0..3 {
            let request = Request::GetStatus {
                path: format!("/test/file{i}.txt"),
            };
            write_message(&mut write_half, &request).await.unwrap();

            let response: Option<Response> = read_message(&mut reader).await.unwrap();
            let response = response.unwrap();
            match response {
                Response::Status { path, .. } => {
                    assert_eq!(path, format!("/test/file{i}.txt"));
                }
                other => panic!("expected Status, got: {other:?}"),
            }
        }

        drop(write_half);
        drop(reader);
        server_handle.abort();
    }

    #[tokio::test]
    async fn client_disconnect_handled_gracefully() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "disconnect.sock");

        let server = IpcServer::bind(&path).unwrap();
        let handler: Arc<dyn RequestHandler> = Arc::new(EchoHandler);

        let server_handle = tokio::spawn({
            let handler = Arc::clone(&handler);
            async move {
                let _ = server.run(handler).await;
            }
        });

        tokio::task::yield_now().await;

        // Connect and immediately disconnect
        let stream = UnixStream::connect(&path).await.unwrap();
        drop(stream);

        // Server should still be running — connect again
        tokio::task::yield_now().await;
        let stream = UnixStream::connect(&path).await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        let request = Request::ForceSync {
            path: "/test".to_string(),
        };
        write_message(&mut write_half, &request).await.unwrap();

        let response: Option<Response> = read_message(&mut reader).await.unwrap();
        assert!(response.is_some());

        drop(write_half);
        drop(reader);
        server_handle.abort();
    }

    #[tokio::test]
    async fn malformed_json_returns_error_keeps_connection() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "malformed.sock");

        let server = IpcServer::bind(&path).unwrap();
        let handler: Arc<dyn RequestHandler> = Arc::new(EchoHandler);

        let server_handle = tokio::spawn({
            let handler = Arc::clone(&handler);
            async move {
                let _ = server.run(handler).await;
            }
        });

        tokio::task::yield_now().await;

        let stream = UnixStream::connect(&path).await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        // Send malformed JSON
        use tokio::io::AsyncWriteExt;
        write_half.write_all(b"not valid json\n").await.unwrap();
        write_half.flush().await.unwrap();

        // Should get an error response
        let response: Option<Response> = read_message(&mut reader).await.unwrap();
        let response = response.unwrap();
        match &response {
            Response::Error { message } => {
                assert!(message.contains("malformed JSON"));
            }
            other => panic!("expected Error response, got: {other:?}"),
        }

        // Connection should still be alive — send valid request
        let request = Request::GetStatus {
            path: "/after/error.txt".to_string(),
        };
        write_message(&mut write_half, &request).await.unwrap();

        let response: Option<Response> = read_message(&mut reader).await.unwrap();
        assert!(matches!(response, Some(Response::Status { .. })));

        drop(write_half);
        drop(reader);
        server_handle.abort();
    }

    #[tokio::test]
    async fn graceful_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "shutdown.sock");

        let server = IpcServer::bind(&path).unwrap();
        let handler: Arc<dyn RequestHandler> = Arc::new(EchoHandler);

        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(async move { server.run(handler).await });

        tokio::task::yield_now().await;

        // Signal shutdown
        shutdown.shutdown();

        // Server should exit cleanly
        let result = server_handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn concurrent_client_connections() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "concurrent.sock");

        let server = IpcServer::bind(&path).unwrap();
        let handler: Arc<dyn RequestHandler> = Arc::new(EchoHandler);

        let server_handle = tokio::spawn({
            let handler = Arc::clone(&handler);
            async move {
                let _ = server.run(handler).await;
            }
        });

        tokio::task::yield_now().await;

        let mut handles = Vec::new();
        for i in 0..5 {
            let path = path.clone();
            handles.push(tokio::spawn(async move {
                let stream = UnixStream::connect(&path).await.unwrap();
                let (read_half, mut write_half) = stream.into_split();
                let mut reader = BufReader::new(read_half);

                let request = Request::GetStatus {
                    path: format!("/concurrent/{i}.txt"),
                };
                write_message(&mut write_half, &request).await.unwrap();

                let response: Option<Response> = read_message(&mut reader).await.unwrap();
                let response = response.unwrap();
                match response {
                    Response::Status { path, .. } => {
                        assert_eq!(path, format!("/concurrent/{i}.txt"));
                    }
                    other => panic!("expected Status, got: {other:?}"),
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        server_handle.abort();
    }

    #[tokio::test]
    async fn handler_error_returns_error_response() {
        let dir = tempfile::tempdir().unwrap();
        let path = temp_socket_path(&dir, "handler_err.sock");

        let server = IpcServer::bind(&path).unwrap();
        let handler: Arc<dyn RequestHandler> = Arc::new(ErrorHandler);

        let server_handle = tokio::spawn({
            let handler = Arc::clone(&handler);
            async move {
                let _ = server.run(handler).await;
            }
        });

        tokio::task::yield_now().await;

        let stream = UnixStream::connect(&path).await.unwrap();
        let (read_half, mut write_half) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        let request = Request::GetStatus {
            path: "/test.txt".to_string(),
        };
        write_message(&mut write_half, &request).await.unwrap();

        let response: Option<Response> = read_message(&mut reader).await.unwrap();
        let response = response.unwrap();
        match response {
            Response::Error { message } => {
                assert!(message.contains("test handler failure"));
            }
            other => panic!("expected Error response, got: {other:?}"),
        }

        drop(write_half);
        drop(reader);
        server_handle.abort();
    }
}
