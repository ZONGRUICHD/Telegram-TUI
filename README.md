# Telegram-TUI

**TDLib + Rust daemon + TUI/CLI** — 完整终端 Telegram 客户端

```text
tg / tg tui  (前端)
      │
      │ Unix Socket, LengthDelimitedCodec, JSON
      ▼
    tgcd     (Rust daemon, tokio)
      │
      │ 自写 tdjson FFI (多客户端 API, @extra UUID)
      ▼
  TDLib C    (libtdjson)
      │
      ▼
Telegram Cloud + 本地 TDLib DB + SQLite Cache
```

## 进程模型

| 程序 | 作用 |
|------|------|
| `tgcd` | 常驻 daemon，TDLib 连接 + SQLite 缓存 + IPC 服务 |
| `tg` | CLI 前端（`tg chats` / `tg send` / …） |
| `tg tui` | TUI 前端（ratatui 终端界面） |

## Crate 布局

```text
crates/
  tdjson/    ← 自写 libtdjson FFI（多客户端 API, @extra 追踪）
  tg-core/   ← 配置、错误、业务模型 (Chat/Message/User/Event)
  tg-ipc/    ← IPC 协议、LengthDelimitedCodec、IpcClient
  tgcd/      ← daemon 二进制
  tg/        ← CLI + TUI 二进制
```

## 技术栈

| 层 | 技术 |
|---|---|
| Telegram 协议 | TDLib / libtdjson |
| Rust 绑定 | 自写 tdjson wrapper (`td_create_client_id` / `td_send` / `td_receive`) |
| 异步运行时 | tokio |
| 序列化 | serde / serde_json |
| 错误处理 | thiserror / anyhow |
| 日志 | tracing |
| CLI | clap |
| TUI | ratatui + crossterm |
| 本地缓存 | SQLite + sqlx |
| IPC | Unix Socket + LengthDelimitedCodec + JSON |
| 配置 | directories + toml + keyring |
| 并发 | dashmap (pending map), uuid (@extra tracking) |

## 快速开始

> 📖 **[完整使用教程](docs/tutorial.md)** — 从安装到日常使用的详细指南

### 一键安装

```bash
git clone https://github.com/ZONGRUICHD/Telegram-TUI.git
cd Telegram-TUI
./scripts/install.sh
```

自动检测系统 → 安装 Rust（如果没有）→ 安装 TDLib（包管理器或源码编译）→ 编译项目 → 安装二进制 → 配置 systemd 服务。

### 卸载

```bash
./scripts/uninstall.sh
```

### 配置

```bash
tg init   # 交互式向导 → ~/.config/tg/config.toml
```

配置示例：

```toml
[telegram]
api_id = 12345678
api_hash = "your_hash"
phone = "+86..."

[tdlib]
database_directory = "~/.local/share/tg/tdlib/db"
files_directory = "~/.local/share/tg/tdlib/files"
use_message_database = true
verbosity = 0

[proxy]
enabled = false
kind = "socks5"
host = "127.0.0.1"
port = 7890

[tui]
enable_mouse = true
message_page_size = 50

[ipc]
socket_path = "/run/user/1000/tg/tgcd.sock"
```

### 使用

```bash
# 启动 daemon
tgcd &
# 或 systemctl --user start tgcd

# 登录
tg login

# CLI
tg chats
tg history <chat-id> --limit 50
tg send <chat-id> "Hello"
tg search <chat-id> "keyword"
tg read <chat-id>
tg download <file-id>
tg me / tg status / tg stop

# TUI
tg tui
```

## TUI 按键

| 键 | 动作 |
|----|------|
| `j` / `↓` | 下一个聊天 |
| `k` / `↑` | 上一个聊天 |
| `i` / `Enter` | 进入输入 |
| `Esc` | 返回聊天列表 |
| `Enter` (输入) | 发送消息 |
| `/q` | 退出 |

## IPC 协议

Length-delimited JSON（4 字节大端长度 + JSON 载荷）：

```text
Client → Daemon:
  ┌──────────┬─────────────────────────────────────────────┐
  │ len (4B) │ {"id":"<uuid>","method":"send_message",...} │
  └──────────┴─────────────────────────────────────────────┘

Daemon → Client:
  ┌──────────┬─────────────────────────────────────────────┐
  │ len (4B) │ {"type":"response","id":"<uuid>","result":…}│
  └──────────┴─────────────────────────────────────────────┘
  ┌──────────┬─────────────────────────────────────────────┐
  │ len (4B) │ {"type":"event","name":"new_message",…}     │
  └──────────┴─────────────────────────────────────────────┘
```

## License

MIT
