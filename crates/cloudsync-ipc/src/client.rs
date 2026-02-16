//! IPC client for CloudSync daemon communication.

use std::path::Path;
use std::time::Duration;

use cloudsync_config::CloudSyncPaths;
use thiserror::Error;
use tokio::io::BufReader;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::time::timeout;

use crate::error::Error;
use crate::messages::{Request, Response};
use crate::transport::{read_message, write_message};

#[cfg(test)]
const CONNECT_TIMEOUT: Duration = Duration::from_millis(250);
#[cfg(not(test))]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

#[cfg(test)]
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(250);
#[cfg(not(test))]
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Client for request/response communication with the daemon over Unix sockets.
#[derive(Debug)]
pub struct IpcClient {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
}

/// IPC client errors with daemon availability and protocol-level categories.
#[derive(Debug, Error)]
pub enum IpcError {
    #[error("daemon is not running")]
    DaemonNotRunning,

    #[error("daemon is unresponsive")]
    DaemonUnresponsive,

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, IpcError>;

impl IpcClient {
    /// Connect to the default daemon IPC socket.
    pub async fn connect() -> Result<Self> {
        let paths = CloudSyncPaths::new().ok_or_else(|| {
            IpcError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "unable to resolve CloudSync paths",
            ))
        })?;

        Self::connect_to(paths.ipc_socket()).await
    }

    /// Connect to a specific socket path.
    pub async fn connect_to(path: impl AsRef<Path>) -> Result<Self> {
        let connect_result = timeout(CONNECT_TIMEOUT, UnixStream::connect(path.as_ref())).await;
        let stream = match connect_result {
            Ok(Ok(stream)) => stream,
            Ok(Err(io_err)) => return Err(map_connect_error(io_err)),
            Err(_) => return Err(IpcError::DaemonUnresponsive),
        };

        let (read_half, write_half) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(read_half),
            writer: write_half,
        })
    }

    /// Send a request and wait for a single response.
    pub async fn request(&mut self, req: &Request) -> Result<Response> {
        write_message(&mut self.writer, req)
            .await
            .map_err(map_transport_error)?;

        let response_result = timeout(
            RESPONSE_TIMEOUT,
            read_message::<Response, _>(&mut self.reader),
        )
        .await;

        match response_result {
            Ok(Ok(Some(response))) => Ok(response),
            Ok(Ok(None)) => Err(IpcError::DaemonUnresponsive),
            Ok(Err(err)) => Err(map_transport_error(err)),
            Err(_) => Err(IpcError::DaemonUnresponsive),
        }
    }

    /// Convenience helper for `Request::GetStatus`.
    pub async fn get_status(&mut self, path: &str) -> Result<Response> {
        self.request(&Request::GetStatus {
            path: path.to_string(),
        })
        .await
    }

    /// Lightweight daemon liveness check on an existing connection.
    pub async fn ping(&mut self) -> Result<bool> {
        let response = self
            .request(&Request::GetStatusBatch { paths: Vec::new() })
            .await?;
        Ok(!matches!(response, Response::Error { .. }))
    }
}

fn map_connect_error(io_err: std::io::Error) -> IpcError {
    if matches!(
        io_err.kind(),
        std::io::ErrorKind::NotFound
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::AddrNotAvailable
            | std::io::ErrorKind::InvalidInput
    ) {
        IpcError::DaemonNotRunning
    } else {
        IpcError::Io(io_err)
    }
}

fn map_transport_error(err: Error) -> IpcError {
    match err {
        Error::Io(io_err) => {
            if matches!(
                io_err.kind(),
                std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::NotConnected
            ) {
                IpcError::DaemonUnresponsive
            } else {
                IpcError::Io(io_err)
            }
        }
        Error::Json(json_err) => IpcError::Protocol(json_err.to_string()),
        Error::ConnectionClosed => IpcError::DaemonUnresponsive,
        Error::MessageTooLarge { size, limit } => {
            IpcError::Protocol(format!("message too large: {size} exceeds {limit}"))
        }
        Error::Handler(message) => IpcError::Protocol(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::io::{AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    use crate::messages::FileStatus;
    use crate::transport::{read_message, write_message};

    fn socket_path(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        (dir, path)
    }

    #[tokio::test]
    async fn connect_to_missing_socket_returns_daemon_not_running() {
        let (_dir, path) = socket_path("missing.sock");
        let err = IpcClient::connect_to(&path).await.unwrap_err();
        assert!(matches!(err, IpcError::DaemonNotRunning));
    }

    #[tokio::test]
    async fn request_roundtrip_with_mock_server() {
        let (_dir, path) = socket_path("roundtrip.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);

            let request: Option<Request> = read_message(&mut reader).await.unwrap();
            assert!(matches!(request, Some(Request::GetStatus { .. })));

            write_message(&mut write_half, &Response::Ok).await.unwrap();
        });

        let mut client = IpcClient::connect_to(&path).await.unwrap();
        let response = client
            .request(&Request::GetStatus {
                path: "/tmp/test.txt".to_string(),
            })
            .await
            .unwrap();

        assert!(matches!(response, Response::Ok));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn malformed_response_returns_protocol_error() {
        let (_dir, path) = socket_path("malformed.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);

            let _: Option<Request> = read_message(&mut reader).await.unwrap();
            write_half.write_all(b"not-json\n").await.unwrap();
            write_half.flush().await.unwrap();
        });

        let mut client = IpcClient::connect_to(&path).await.unwrap();
        let err = client
            .request(&Request::GetStatus {
                path: "/tmp/test.txt".to_string(),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, IpcError::Protocol(_)));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn get_status_sends_get_status_request() {
        let (_dir, path) = socket_path("get_status.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);

            let request: Option<Request> = read_message(&mut reader).await.unwrap();
            match request {
                Some(Request::GetStatus { path }) => assert_eq!(path, "/tmp/x.txt"),
                other => panic!("expected GetStatus, got {other:?}"),
            }

            write_message(&mut write_half, &Response::Ok).await.unwrap();
        });

        let mut client = IpcClient::connect_to(&path).await.unwrap();
        let response = client.get_status("/tmp/x.txt").await.unwrap();
        assert!(matches!(response, Response::Ok));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn ping_uses_empty_status_batch_request() {
        let (_dir, path) = socket_path("ping.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);

            let request: Option<Request> = read_message(&mut reader).await.unwrap();
            match request {
                Some(Request::GetStatusBatch { paths }) => assert!(paths.is_empty()),
                other => panic!("expected GetStatusBatch, got {other:?}"),
            }

            write_message(
                &mut write_half,
                &Response::StatusBatch {
                    statuses: Vec::<FileStatus>::new(),
                },
            )
            .await
            .unwrap();
        });

        let mut client = IpcClient::connect_to(&path).await.unwrap();
        let is_alive = client.ping().await.unwrap();
        assert!(is_alive);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn ping_returns_false_when_server_replies_with_error() {
        let (_dir, path) = socket_path("ping_error.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, mut write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);

            let _: Option<Request> = read_message(&mut reader).await.unwrap();
            write_message(
                &mut write_half,
                &Response::Error {
                    message: "unhealthy".to_string(),
                },
            )
            .await
            .unwrap();
        });

        let mut client = IpcClient::connect_to(&path).await.unwrap();
        let is_alive = client.ping().await.unwrap();
        assert!(!is_alive);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_to_socket_then_server_disconnects_reports_unresponsive() {
        let (_dir, path) = socket_path("disconnect.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (_read_half, _write_half) = stream.into_split();
            // Drop immediately without sending a response.
        });

        let mut client = IpcClient::connect_to(&path).await.unwrap();
        let err = client
            .request(&Request::GetStatus {
                path: "/tmp/y.txt".to_string(),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, IpcError::DaemonUnresponsive));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn request_times_out_when_server_does_not_reply() {
        let (_dir, path) = socket_path("timeout.sock");
        let listener = UnixListener::bind(&path).unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read_half, _write_half) = stream.into_split();
            let mut reader = BufReader::new(read_half);
            let _: Option<Request> = read_message(&mut reader).await.unwrap();

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        });

        let mut client = IpcClient::connect_to(&path).await.unwrap();
        let err = client
            .request(&Request::GetStatus {
                path: "/tmp/z.txt".to_string(),
            })
            .await
            .unwrap_err();

        assert!(matches!(err, IpcError::DaemonUnresponsive));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn connect_to_existing_socket_path_with_no_listener_returns_daemon_not_running() {
        let (_dir, path) = socket_path("stale.sock");
        let listener = UnixListener::bind(&path).unwrap();
        drop(listener);

        let err = IpcClient::connect_to(&path).await.unwrap_err();
        assert!(matches!(err, IpcError::DaemonNotRunning));
    }
}
