use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tg_ipc::{client::IpcClient, protocol::RpcError, transport::Listener};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[tokio::test]
async fn request_ids_events_errors_and_disconnect() {
    let root = std::env::temp_dir().join(format!("tg-rpc-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("rpc.sock");
    let mut listener = Listener::bind(&path).unwrap();
    let server = tokio::spawn(async move {
        let stream = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
        let frame = framed.next().await.unwrap().unwrap();
        let request: serde_json::Value = serde_json::from_slice(&frame).unwrap();
        for message in [
            json!({"type":"event","name":"updateNewMessage","data":{}}),
            json!({"type":"response","id":"unrelated","result":"wrong"}),
            json!({"type":"response","id":request["id"],"result":{"text":"中文"}}),
        ] {
            framed.send(Bytes::from(message.to_string())).await.unwrap();
        }
        let frame = framed.next().await.unwrap().unwrap();
        let request: serde_json::Value = serde_json::from_slice(&frame).unwrap();
        framed.send(Bytes::from(json!({"type":"response","id":request["id"],"error":{"code":429,"message":"FLOOD_WAIT_5"}}).to_string())).await.unwrap();
    });
    let mut client = IpcClient::connect(&path).await.unwrap();
    assert_eq!(
        client.call("test", json!({})).await.unwrap()["text"],
        "中文"
    );
    let error = client.call("error", json!({})).await.unwrap_err();
    assert_eq!(error.downcast_ref::<RpcError>().unwrap().code, 429);
    assert!(client.read_message().await.is_err());
    server.await.unwrap();
    std::fs::remove_dir(root).unwrap();
}
