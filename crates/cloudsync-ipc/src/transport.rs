//! Newline-delimited JSON message framing over async streams.

use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::error::{Error, Result};

/// Maximum message size: 1 MB.
pub const MAX_MESSAGE_SIZE: usize = 1_048_576;

/// Read a single newline-delimited JSON message from the stream.
///
/// Returns `Ok(None)` on clean EOF (no partial data).
pub async fn read_message<T, R>(reader: &mut BufReader<R>) -> Result<Option<T>>
where
    T: DeserializeOwned,
    R: tokio::io::AsyncRead + Unpin,
{
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;

    if bytes_read == 0 {
        return Ok(None);
    }

    if line.len() > MAX_MESSAGE_SIZE {
        return Err(Error::MessageTooLarge {
            size: line.len(),
            limit: MAX_MESSAGE_SIZE,
        });
    }

    let message = serde_json::from_str(line.trim_end())?;
    Ok(Some(message))
}

/// Write a single newline-delimited JSON message to the stream.
pub async fn write_message<T, W>(writer: &mut W, message: &T) -> Result<()>
where
    T: Serialize,
    W: AsyncWrite + Unpin,
{
    let mut json = serde_json::to_string(message)?;
    json.push('\n');
    writer.write_all(json.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestMsg {
        action: String,
        value: i32,
    }

    #[tokio::test]
    async fn roundtrip_message() {
        let (client, server) = tokio::io::duplex(1024);
        let (_reader_half, mut writer_half) = tokio::io::split(server);
        let (client_reader, _client_writer) = tokio::io::split(client);

        let msg = TestMsg {
            action: "test".to_string(),
            value: 42,
        };

        write_message(&mut writer_half, &msg).await.unwrap();
        drop(writer_half);

        let mut reader = BufReader::new(client_reader);
        let received: Option<TestMsg> = read_message(&mut reader).await.unwrap();
        assert_eq!(received, Some(msg));
    }

    #[tokio::test]
    async fn eof_returns_none() {
        let (client, server) = tokio::io::duplex(1024);
        drop(server);

        let mut reader = BufReader::new(client);
        let result: Option<TestMsg> = read_message(&mut reader).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn malformed_json_returns_error() {
        let (client, server) = tokio::io::duplex(1024);
        let (_reader_half, mut writer_half) = tokio::io::split(server);

        tokio::io::AsyncWriteExt::write_all(&mut writer_half, b"not valid json\n")
            .await
            .unwrap();
        drop(writer_half);

        let (client_reader, _client_writer) = tokio::io::split(client);
        let mut reader = BufReader::new(client_reader);
        let result: Result<Option<TestMsg>> = read_message(&mut reader).await;
        assert!(matches!(result, Err(Error::Json(_))));
    }

    #[tokio::test]
    async fn oversized_message_returns_error() {
        let (client, server) = tokio::io::duplex(MAX_MESSAGE_SIZE + 1024);
        let (_reader_half, mut writer_half) = tokio::io::split(server);

        // Create a message larger than MAX_MESSAGE_SIZE
        let big_value = "x".repeat(MAX_MESSAGE_SIZE + 100);
        let mut payload = format!(r#"{{"action":"big","value":{}}}"#, 0);
        payload.push_str(&big_value);
        payload.push('\n');

        tokio::io::AsyncWriteExt::write_all(&mut writer_half, payload.as_bytes())
            .await
            .unwrap();
        drop(writer_half);

        let (client_reader, _client_writer) = tokio::io::split(client);
        let mut reader = BufReader::new(client_reader);
        let result: Result<Option<TestMsg>> = read_message(&mut reader).await;
        assert!(matches!(result, Err(Error::MessageTooLarge { .. })));
    }

    #[tokio::test]
    async fn multiple_messages_on_same_stream() {
        let (client, mut server) = tokio::io::duplex(4096);

        let msg1 = TestMsg {
            action: "first".to_string(),
            value: 1,
        };
        let msg2 = TestMsg {
            action: "second".to_string(),
            value: 2,
        };

        write_message(&mut server, &msg1).await.unwrap();
        write_message(&mut server, &msg2).await.unwrap();
        drop(server);

        let mut reader = BufReader::new(client);

        let received1: Option<TestMsg> = read_message(&mut reader).await.unwrap();
        assert_eq!(received1, Some(msg1));

        let received2: Option<TestMsg> = read_message(&mut reader).await.unwrap();
        assert_eq!(received2, Some(msg2));

        let received3: Option<TestMsg> = read_message(&mut reader).await.unwrap();
        assert!(received3.is_none());
    }
}
