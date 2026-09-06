//! IPC client — connects to tgcd over a Unix socket.
//!
//! Uses `LengthDelimitedCodec` for framing.

use std::path::Path;

use anyhow::Result;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value as JsonValue;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use crate::codec::MAX_FRAME_LEN;
use crate::protocol::{Request, ServerMessage};

type IpcFramed = Framed<crate::transport::Stream, LengthDelimitedCodec>;

pub struct IpcClient {
    framed: IpcFramed,
}

impl IpcClient {
    pub async fn connect(socket_path: &Path) -> Result<Self> {
        let stream = crate::transport::connect(socket_path).await?;
        let codec = LengthDelimitedCodec::builder()
            .max_frame_length(MAX_FRAME_LEN)
            .big_endian()
            .new_codec();
        Ok(Self {
            framed: Framed::new(stream, codec),
        })
    }

    /// Split into independent reader and writer halves.
    pub fn split(self) -> (IpcWriter, IpcReader) {
        let (sink, stream) = self.framed.split();
        (IpcWriter { sink }, IpcReader { stream })
    }

    /// Send a request and wait for the matching response (by UUID).
    pub async fn call(&mut self, method: &str, params: JsonValue) -> Result<JsonValue> {
        self.call_with_timeout(method, params, std::time::Duration::from_secs(35))
            .await
    }

    pub async fn call_with_timeout(
        &mut self,
        method: &str,
        params: JsonValue,
        timeout: std::time::Duration,
    ) -> Result<JsonValue> {
        tokio::time::timeout(timeout, self.call_inner(method, params))
            .await
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "IPC 请求超时，结果未知；发送操作请先核对历史记录，勿自动重试",
                )
            })?
    }

    async fn call_inner(&mut self, method: &str, params: JsonValue) -> Result<JsonValue> {
        let id = uuid::Uuid::new_v4().to_string();
        let req = Request {
            id: id.clone(),
            method: method.to_string(),
            params,
        };
        self.send_request(&req).await?;

        loop {
            let msg = self.read_message().await?;
            match msg {
                ServerMessage::Response(resp) if resp.id == id => {
                    if let Some(err) = resp.error {
                        return Err(anyhow::Error::new(err));
                    }
                    return Ok(resp.result.unwrap_or(JsonValue::Null));
                }
                _ => continue,
            }
        }
    }

    pub async fn send_request(&mut self, req: &Request) -> Result<()> {
        let payload = serde_json::to_vec(req)?;
        self.framed.send(Bytes::from(payload)).await?;
        Ok(())
    }

    /// Send a raw JSON value (for special cases like login flow).
    pub async fn send_raw(&mut self, value: JsonValue) -> Result<()> {
        let payload = serde_json::to_vec(&value)?;
        self.framed.send(Bytes::from(payload)).await?;
        Ok(())
    }

    pub async fn read_message(&mut self) -> Result<ServerMessage> {
        let frame = self
            .framed
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("connection closed"))??;
        Ok(serde_json::from_slice(&frame)?)
    }
}

// ── Split halves ───────────────────────────────────────────────────

pub struct IpcWriter {
    sink: futures_util::stream::SplitSink<IpcFramed, Bytes>,
}

impl IpcWriter {
    pub async fn send_request(&mut self, req: &Request) -> Result<()> {
        let payload = serde_json::to_vec(req)?;
        self.sink.send(Bytes::from(payload)).await?;
        Ok(())
    }
}

pub struct IpcReader {
    stream: futures_util::stream::SplitStream<IpcFramed>,
}

impl IpcReader {
    pub async fn read_message(&mut self) -> Result<ServerMessage> {
        let frame = self
            .stream
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("connection closed"))??;
        Ok(serde_json::from_slice(&frame)?)
    }
}
