//! TDLib interaction layer.
//!
//! Uses `tg_tdjson::TdClient` (multi-client API with @extra tracking).

use serde_json::Value as JsonValue;

/// Send a TDLib query and wait for the response.
pub async fn query(td: &tg_tdjson::TdClient, query: JsonValue) -> anyhow::Result<JsonValue> {
    let response = td.send(query).await?;
    if response["@type"] == "error" {
        let code = response["code"].as_i64().unwrap_or(-1) as i32;
        let message = if code == 406 {
            "Telegram 拒绝了请求".into()
        } else {
            response["message"]
                .as_str()
                .unwrap_or("未知 Telegram 错误")
                .to_owned()
        };
        return Err(tg_core::error::TgError::Tdlib { code, message }.into());
    }
    Ok(response)
}

/// Fire-and-forget TDLib query.
pub fn notify(td: &tg_tdjson::TdClient, query: JsonValue) {
    td.send_no_wait(query);
}
