//! Multi-client TDLib JSON wrapper (`libtdjson`).
//!
//! Uses the multi-client API: `td_create_client_id`, `td_send`, `td_receive`.
//! Supports `@extra` UUID tracking with oneshot channels for request-response matching.
//!
//! # Architecture
//!
//! ```text
//! TdClient (per-client)
//!   ├─ client_id: i32
//!   ├─ pending: DashMap<String, oneshot::Sender<JsonValue>>   ← @extra → response
//!   └─ updates_tx: broadcast::Sender<JsonValue>              ← push updates
//!
//! receive_loop (shared thread)
//!   └─ td_receive(timeout) → parse → route by @extra
//! ```

use std::ffi::{CStr, CString};
use std::os::raw::c_int;
use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value as JsonValue;
use tokio::sync::{broadcast, oneshot};
use tracing::warn;
mod library;
pub use library::load as load_library;

// ── FFI declarations ───────────────────────────────────────────────

// ── Low-level helpers ──────────────────────────────────────────────

/// Set TDLib log verbosity. Call once at startup.
pub fn set_log_verbosity(level: i32) {
    unsafe { (library::api().verbosity)(level) }
}

/// Execute a synchronous TDLib function (rarely used).
pub fn execute(query: &str) -> Option<String> {
    let c = CString::new(query).ok()?;
    let ptr = unsafe { (library::api().execute)(c.as_ptr()) };
    if ptr.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

// ── Shared receive loop state ──────────────────────────────────────

/// Shared state for all TdClient instances.
/// One receive loop thread serves all clients.
struct ReceiveState {
    /// Maps `@extra` string → oneshot sender for pending requests.
    pending: DashMap<String, oneshot::Sender<JsonValue>>,
    /// Broadcast channel for update events (no @extra).
    updates_tx: broadcast::Sender<JsonValue>,
}

static RECEIVE_STATE: std::sync::OnceLock<Arc<ReceiveState>> = std::sync::OnceLock::new();

/// Initialize the global receive loop. Call once at startup.
/// Returns the broadcast receiver for updates.
pub fn init(updates_tx: broadcast::Sender<JsonValue>) {
    let state = Arc::new(ReceiveState {
        pending: DashMap::new(),
        updates_tx,
    });
    if RECEIVE_STATE.set(state.clone()).is_err() {
        panic!("tdjson receive loop already initialized");
    }

    std::thread::Builder::new()
        .name("tdlib-recv".into())
        .spawn(move || receive_loop(state))
        .expect("failed to spawn tdlib receive loop");
}

fn get_state() -> &'static Arc<ReceiveState> {
    RECEIVE_STATE
        .get()
        .expect("tdjson not initialized — call init() first")
}

// ── Receive loop ───────────────────────────────────────────────────

fn receive_loop(state: Arc<ReceiveState>) {
    tracing::info!("TDLib receive loop started");
    loop {
        let ptr = unsafe { (library::api().receive)(1.0) };
        if ptr.is_null() {
            continue; // timeout
        }

        let raw = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();

        let val: JsonValue = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                warn!("tdjson: failed to parse response: {e}");
                continue;
            }
        };

        // Route by @extra — can be string (our UUIDs) or number (TDLib internal)
        let extra_key = val
            .get("@extra")
            .and_then(|v| v.as_str().map(String::from))
            .or_else(|| {
                val.get("@extra")
                    .and_then(|v| v.as_i64())
                    .map(|n| n.to_string())
            });

        if let Some(ref extra) = extra_key {
            if let Some((_, sender)) = state.pending.remove(extra) {
                let _ = sender.send(val);
                continue;
            }
            // @extra present but no matching pending — stale response, log and skip
            tracing::debug!("tdjson: stale @extra={extra}, discarding");
            continue;
        }

        // No @extra or no matching pending — it's an update
        let _ = state.updates_tx.send(val);
    }
}

// ── TdClient — per-client handle ───────────────────────────────────

/// A handle to a TDLib client instance.
///
/// Each `TdClient` has its own `client_id` but shares the global
/// receive loop and pending map.
#[derive(Clone)]
pub struct TdClient {
    client_id: c_int,
}

impl TdClient {
    /// Create a new TDLib client (calls `td_create_client_id`).
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let client_id = unsafe { (library::api().create)() };
        Self { client_id }
    }

    /// Send a request and wait for the response (matched by `@extra`).
    /// Times out after 30 seconds.
    pub async fn send(&self, mut query: JsonValue) -> anyhow::Result<JsonValue> {
        let extra = uuid::Uuid::new_v4().to_string();
        query["@extra"] = serde_json::Value::String(extra.clone());

        let (tx, rx) = oneshot::channel();
        get_state().pending.insert(extra.clone(), tx);
        let _pending = PendingRequest(extra.clone());

        let c_query = CString::new(query.to_string())?;
        unsafe { (library::api().send)(self.client_id, c_query.as_ptr()) }

        match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => {
                get_state().pending.remove(&extra);
                anyhow::bail!("TDLib response channel closed for @extra={extra}")
            }
            Err(_) => {
                get_state().pending.remove(&extra);
                anyhow::bail!("TDLib request timed out after 30s for @extra={extra}")
            }
        }
    }

    /// Fire-and-forget: send without waiting for response.
    pub fn send_no_wait(&self, query: JsonValue) {
        let c_query = match CString::new(query.to_string()) {
            Ok(c) => c,
            Err(_) => return,
        };
        unsafe { (library::api().send)(self.client_id, c_query.as_ptr()) }
    }

    /// Get the raw client_id.
    pub fn client_id(&self) -> c_int {
        self.client_id
    }
}

// ── Convenience: subscribe to updates ──────────────────────────────

/// Subscribe to the global update broadcast channel.
pub fn subscribe_updates() -> broadcast::Receiver<JsonValue> {
    get_state().updates_tx.subscribe()
}

struct PendingRequest(String);
impl Drop for PendingRequest {
    fn drop(&mut self) {
        get_state().pending.remove(&self.0);
    }
}
