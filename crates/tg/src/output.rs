use anyhow::Result;
use serde_json::Value;
use std::io::Write;

pub fn json(value: &Value) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, value)?;
    writeln!(out)?;
    out.flush()?;
    Ok(())
}
pub fn success(result: &Value, machine: bool) -> Result<()> {
    if machine {
        return json(&serde_json::json!({"schema_version":1,"ok":true,"result":result}));
    }
    let mut out = std::io::stdout().lock();
    if let Some(chats) = result["chats"].as_array() {
        for chat in chats {
            writeln!(
                out,
                "{}  {}  未读 {}",
                chat["id"],
                safe(chat["title"].as_str().unwrap_or("?")),
                chat["unread_count"]
            )?;
        }
    } else if let Some(messages) = result["messages"].as_array() {
        for m in messages.iter().rev() {
            writeln!(
                out,
                "{}  {}  #{}\n  {}",
                time(m["date"].as_i64().unwrap_or(0), "%m-%d %H:%M"),
                sender(m),
                m["id"],
                message_text(m)
            )?;
        }
        if !result["next_from_message_id"].is_null() {
            writeln!(out, "下一页 --before {}", result["next_from_message_id"])?;
        }
    } else {
        writeln!(out, "{}", safe(&serde_json::to_string_pretty(result)?))?;
    }
    Ok(())
}
/// Prevent terminal escape/control injection from untrusted remote message content.
pub fn safe(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}
pub fn time(ts: i64, pattern: &str) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.with_timezone(&chrono::Local).format(pattern).to_string())
        .unwrap_or_else(|| "--:--".into())
}
pub fn sender(m: &Value) -> String {
    if m["is_outgoing"].as_bool().unwrap_or(false) {
        return "你".into();
    }
    if let Some(name) = m["sender_name"].as_str() {
        return safe(name);
    }
    if let Some(id) = m["sender_id"]["user_id"].as_i64() {
        return format!("用户 {id}");
    }
    "频道".into()
}
pub fn message_text(m: &Value) -> String {
    let content = &m["content"];
    let caption = content["caption"]["text"].as_str().unwrap_or("");
    let kind = content["@type"].as_str().unwrap_or("");
    let label = match kind {
        "messageText" => return safe(content["text"]["text"].as_str().unwrap_or("")),
        "messagePhoto" => "图片".to_string(),
        "messageVideo" => "视频".to_string(),
        "messageDocument" => format!(
            "文件 · {}",
            content["document"]["file_name"].as_str().unwrap_or("附件")
        ),
        "messageSticker" => format!(
            "贴纸 {}",
            content["sticker"]["emoji"].as_str().unwrap_or("")
        ),
        "messageVoiceNote" => "语音消息".into(),
        "messageVideoNote" => "视频消息".into(),
        "messageAudio" => format!(
            "音频 · {}",
            content["audio"]["title"].as_str().unwrap_or("")
        ),
        "messageAnimation" => "GIF 动画".into(),
        "messagePoll" => format!(
            "投票 · {}",
            content["poll"]["question"]["text"].as_str().unwrap_or("")
        ),
        "messageLocation" => format!(
            "位置 · {}, {}",
            content["location"]["latitude"], content["location"]["longitude"]
        ),
        "messageContact" => format!(
            "联系人 · {}",
            content["contact"]["first_name"].as_str().unwrap_or("")
        ),
        other => format!("[{}]", other.strip_prefix("message").unwrap_or(other)),
    };
    let label = if let Some(id) = file_id(m) {
        format!("{label} · 文件 ID {id}")
    } else {
        label
    };
    safe(&if caption.is_empty() {
        label
    } else {
        format!("{label}\n{caption}")
    })
}
pub fn file_id(m: &Value) -> Option<i64> {
    let c = &m["content"];
    if c["@type"] == "messagePhoto" {
        return c["photo"]["sizes"].as_array()?.last()?["photo"]["id"].as_i64();
    }
    for (outer, inner) in [
        ("document", "document"),
        ("video", "video"),
        ("audio", "audio"),
        ("voice_note", "voice"),
        ("video_note", "video"),
        ("animation", "animation"),
        ("sticker", "sticker"),
    ] {
        if let Some(id) = c[outer][inner]["id"].as_i64() {
            return Some(id);
        }
    }
    None
}
