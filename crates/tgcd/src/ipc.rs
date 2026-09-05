//! Framed local RPC with update streaming and acknowledged shutdown.
use crate::handler::AppState;
use anyhow::Result;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use std::{path::Path, sync::Arc};
use tg_ipc::{
    codec::MAX_FRAME_LEN,
    protocol::{Request, ServerMessage},
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

pub async fn run(path: &Path, state: AppState) -> Result<()> {
    let mut listener = tg_ipc::transport::Listener::bind(path)?;
    let mut shutdown = state.shutdown_tx.subscribe();
    let state = Arc::new(state);
    loop {
        tokio::select! {
            result = listener.accept() => {
                let stream = result?;
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_client(stream, state).await { tracing::debug!("IPC: {error}"); }
                });
            }
            _ = shutdown.changed() => break,
            _ = tokio::signal::ctrl_c() => break,
        }
    }
    Ok(())
}

async fn handle_client(stream: tg_ipc::transport::Stream, state: Arc<AppState>) -> Result<()> {
    let codec = LengthDelimitedCodec::builder()
        .max_frame_length(MAX_FRAME_LEN)
        .big_endian()
        .new_codec();
    let (mut writer, mut reader) = Framed::new(stream, codec).split();
    let mut updates = state.updates_tx.subscribe();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(Bytes, bool)>(64);
    let shutdown = state.shutdown_tx.clone();
    let write_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                response = rx.recv() => match response {
                    Some((bytes, stop)) => {
                        if writer.send(bytes).await.is_err() { break; }
                        if stop { shutdown.send_replace(true); break; }
                    }
                    None => break,
                },
                update = updates.recv() => {
                    let msg = match update {
                        Ok(data) => serde_json::json!({"type":"event","name":data["@type"],"data":data}),
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => serde_json::json!({"type":"event","name":"resync_required","data":{"dropped":n}}),
                        Err(_) => break,
                    };
                    if writer.send(Bytes::from(serde_json::to_vec(&msg).unwrap_or_default())).await.is_err() { break; }
                }
            }
        }
    });
    while let Some(frame) = reader.next().await {
        let frame = match frame {
            Ok(frame) => frame,
            Err(_) => break,
        };
        let request: Request = match serde_json::from_slice(&frame) {
            Ok(request) => request,
            Err(_) => {
                let message = serde_json::json!({"type":"response","id":"","error":{"code":-32700,"message":"无效 JSON 请求"}});
                if tx
                    .send((Bytes::from(serde_json::to_vec(&message)?), false))
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
        };
        let stop = request.method == "shutdown";
        let response = crate::handler::handle_request(request, &state).await;
        let stop = stop && response.error.is_none();
        if tx
            .send((
                Bytes::from(serde_json::to_vec(&ServerMessage::Response(response))?),
                stop,
            ))
            .await
            .is_err()
        {
            break;
        }
        if stop {
            break;
        }
    }
    drop(tx);
    let _ = write_task.await;
    Ok(())
}
