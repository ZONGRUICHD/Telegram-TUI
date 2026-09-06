# Telegram-TUI 使用教程

> TDLib + Rust 常驻 Daemon + CLI/TUI 前端的完整终端 Telegram 客户端

---

## 目录

- [一、项目简介](#一项目简介)
- [二、安装](#二安装)
- [三、申请 API 凭证](#三申请-api-凭证)
- [四、初始化配置](#四初始化配置)
- [五、启动 Daemon](#五启动-daemon)
- [六、登录 Telegram](#六登录-telegram)
- [七、CLI 命令详解](#七cli-命令详解)
- [八、TUI 终端界面](#八tui-终端界面)
- [九、代理配置](#九代理配置)
- [十、配置文件参考](#十配置文件参考)
- [十一、运维管理](#十一运维管理)
- [十二、目录结构](#十二目录结构)
- [十三、卸载](#十三卸载)
- [十四、常见问题](#十四常见问题)

---

## 一、项目简介

Telegram-TUI 是一个运行在终端里的 Telegram 客户端，架构如下：

```text
tg (CLI) / tg tui (终端界面)
      │
      │ Unix Socket, LengthDelimitedCodec, JSON
      ▼
    tgcd (Rust 常驻 Daemon)
      │
      │ 自写 tdjson FFI (多客户端 API, @extra UUID)
      ▼
  TDLib / libtdjson (Telegram 官方客户端库)
      │
      ▼
Telegram Cloud + 本地 TDLib 数据库 + SQLite 消息缓存
```

**核心特性：**

- 常驻 Daemon — 登录状态持久保持，消息更新实时接收
- CLI 前端 — 一行命令完成发消息、查聊天、搜索等操作
- TUI 前端 — ratatui 驱动的全屏终端聊天界面
- SQLite 缓存 — 消息本地缓存，查询速度快
- 代理支持 — socks5 / http / mtproto
- systemd 集成 — 开机自启、崩溃自动重启

---

## 二、安装

### 2.1 一键安装（推荐）

```bash
git clone https://github.com/ZONGRUICHD/Telegram-TUI.git
cd Telegram-TUI
./scripts/install.sh
```

脚本自动完成：

1. 检测操作系统（Arch / Debian / Ubuntu / macOS）
2. 安装 Rust 工具链（如果没有）
3. 安装系统依赖（cmake、git、gcc 等）
4. 安装 TDLib（先找包管理器，找不到就源码编译）
5. 编译项目（`cargo build --release`）
6. 安装二进制到 `~/.cargo/bin/`
7. 配置 systemd user service

**环境变量控制：**

| 变量 | 说明 |
|------|------|
| `PREFIX=/usr/local/bin` | 自定义安装目录 |
| `BUILD_TDLIB=1` | 强制从源码编译 TDLib |
| `SKIP_SYSTEMD=1` | 跳过 systemd 服务安装 |

示例：

```bash
# 安装到 /usr/local/bin，强制编译 TDLib
PREFIX=/usr/local/bin BUILD_TDLIB=1 ./scripts/install.sh
```

### 2.2 手动安装

```bash
# 1. 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# 2. 安装 TDLib
# Arch
sudo pacman -S tdlib
# Ubuntu/Debian
sudo apt install libtd-dev
# macOS
brew install tdlib
# 或从源码编译
./scripts/build-tdlib.sh

# 3. 编译
cargo build --release

# 4. 安装
cp target/release/tg target/release/tgcd ~/.cargo/bin/
```

### 2.3 验证安装

```bash
tg --version
tgcd --version
```

---

## 三、申请 API 凭证

每个 Telegram 客户端都需要自己的 `api_id` 和 `api_hash`。

1. 打开浏览器访问 **https://my.telegram.org**
2. 输入你的**手机号**（国际格式，如 `+8613800138000`）
3. 输入 Telegram 发来的**验证码**
4. 登录后点击 **API development tools**
5. 填写表单：
   - **App title**：随意填写，如 `My Telegram CLI`
   - **Short name**：随意填写，如 `mycli`
   - **URL**：可留空
   - **Platform**：选 `Desktop`
   - **Description**：可留空
6. 点击 **Create application**
7. 页面会显示：
   - `App api_id`：一串数字（如 `12345678`）
   - `App api_hash`：一串 32 位十六进制字符串（如 `abcdef1234567890abcdef1234567890`）

> **重要：** 这两个值是你的应用凭证，不要分享给他人。

---

## 四、初始化配置

```bash
tg init
```

交互式输入：

```
🚀  Telegram CLI — Initial Setup

API ID (https://my.telegram.org/apps): 12345678
API Hash: abcdef1234567890abcdef1234567890
Phone (+86..., or blank): +8613800138000

🔑  Credentials stored in system keyring.

✅  Saved to /home/user/.config/tg/config.toml
   Next: tgcd & && tg login
```

**说明：**

- `api_id` 和 `api_hash` 同时保存在配置文件和系统钥匙串中
- `phone` 可以留空，登录时再输入
- 配置文件路径：`~/.config/tg/config.toml`

---

## 五、启动 Daemon

### 方式一：systemd（推荐）

```bash
# 启动
systemctl --user start tgcd

# 查看状态
systemctl --user status tgcd

# 查看日志
journalctl --user -u tgcd -f
```

systemd 管理的好处：
- 开机自动启动
- 崩溃后 5 秒自动重启
- 日志自动收集

### 方式二：手动启动

```bash
# 前台运行（调试用）
tgcd

# 后台运行
tgcd &

# 查看日志
RUST_LOG=debug tgcd
```

### 验证 Daemon 运行

```bash
tg status
```

输出：

```
Socket: /run/user/1000/tg/tgcd.sock
```

---

## 六、登录 Telegram

```bash
tg login
```

### 已有 Session（自动恢复）

如果之前已经登录过，Daemon 会自动恢复 session：

```
🔐  Logging in…

⏳  Waiting for auth events…

✅  Logged in!
```

### 首次登录

```
🔐  Logging in…

⏳  Waiting for auth events…

📱  Phone number: +8613800138000
🔑  Code: 12345                          ← Telegram 发到你手机/其他设备的验证码
🔒  2FA password: xxx                    ← 如果开了两步验证才需要这一步
✅  Logged in!
```

**说明：**

- 验证码会发送到你 Telegram 已登录的设备上（手机 App 或其他客户端）
- 如果开启了**两步验证**（Two-Step Verification），需要输入密码
- 登录后 session 保存在 `~/.local/share/tg/tdlib/`，下次启动 Daemon 自动恢复

---

## 七、CLI 命令详解

### 7.1 查看聊天列表

```bash
tg chats              # 默认显示 20 个
tg chats -l 50        # 显示 50 个
```

输出：

```
📋  15 chats:

    1. Alice              [123456789]
    2. (3) Linux Group    [987654321]
    3. (1) Bot Dev        [111222333]
    4. Saved Messages     [444555666]
```

- 括号里的数字是**未读消息数**
- 方括号里是 **chat_id**，后续命令用它来指定聊天

### 7.2 查看消息

```bash
tg history <chat-id>              # 默认 50 条
tg history <chat-id> -l 100       # 指定 100 条
tg history 123456789 -l 20        # 查看最近 20 条
```

输出：

```
[2026-06-14 10:23] user#123456 #778899
  你好，最近怎么样？

[2026-06-14 10:24] user#654321 #779000
  还不错，在写一个项目

[2026-06-14 10:25] user#123456 #779001
  什么项目？分享一下
```

### 7.3 发送消息

```bash
tg send <chat-id> "消息内容"
tg send 123456789 "Hello, world!"
tg send 123456789 "你好 世界"         # 支持中文
tg send @username "Hi there"         # 支持 @用户名
```

### 7.4 搜索消息

```bash
tg search <chat-id> "关键词"
tg search 123456789 "error"
tg search 123456789 "会议" -l 50
```

### 7.5 标记已读

```bash
tg read <chat-id>
tg read 123456789
```

### 7.6 转发消息

```bash
tg forward <源chat-id> <目标chat-id> <消息id>
tg forward 123456789 987654321 778899
```

### 7.7 删除消息

```bash
tg delete <chat-id> <消息id>
tg delete 123456789 778899
```

> 只能删除自己发的消息（除非你是群管理员）。

### 7.8 下载文件

```bash
tg download <file-id>
```

### 7.9 查看账号信息

```bash
tg me
```

输出：

```
👤  Alice Smith (@alice)  id=123456789
```

### 7.10 Daemon 状态

```bash
tg status
```

---

## 八、TUI 终端界面

```bash
tg tui
```

### 界面布局

```text
┌ 📱 tg tui │ j/k navigate │ i input │ /q quit ──────────────────────┐
┌ Chats ───────────────┐┌ Messages ──────────────────────────────────┐
│ (2) Alice            ││ [10:23] Alice: 你好，最近怎么样？            │
│ Linux Group          ││ [10:24] Me: 还不错，在写一个项目             │
│ Bot Dev              ││ [10:25] Alice: 什么项目？分享一下             │
│ (1) Saved Messages   ││                                            │
│                      ││                                            │
└──────────────────────┘└────────────────────────────────────────────┘
┌ Input ──────────────────────────────────────────────────────────────┐
│ > █                                                                 │
└─────────────────────────────────────────────────────────────────────┘
 Loading…
```

### 按键操作

| 模式 | 按键 | 功能 |
|------|------|------|
| 聊天列表 | `j` / `↓` | 选择下一个聊天 |
| 聊天列表 | `k` / `↑` | 选择上一个聊天 |
| 聊天列表 | `Enter` / `i` | 进入输入模式 |
| 聊天列表 | `/` | 进入输入模式（命令模式） |
| 聊天列表 | `q` / `Esc` | 退出 TUI |
| 输入模式 | `Enter` | 发送消息 |
| 输入模式 | `Esc` | 返回聊天列表 |
| 输入模式 | `Backspace` | 删除字符 |
| 输入模式 | `/q` | 退出 TUI |

### TUI 内命令

在输入模式下输入以 `/` 开头的命令：

| 命令 | 功能 |
|------|------|
| `/q` | 退出 TUI |

---

## 九、代理配置

编辑 `~/.config/tg/config.toml`：

### SOCKS5 代理

```toml
[proxy]
enabled = true
kind = "socks5"
host = "127.0.0.1"
port = 7890
```

### HTTP 代理

```toml
[proxy]
enabled = true
kind = "http"
host = "127.0.0.1"
port = 8080
```

### 需要认证的代理

```toml
[proxy]
enabled = true
kind = "socks5"
host = "127.0.0.1"
port = 1080
username = "myuser"
password = "mypass"
```

### MTProto 代理

```toml
[proxy]
enabled = true
kind = "mtproto"
host = "proxy.example.com"
port = 443
```

修改配置后重启 Daemon：

```bash
systemctl --user restart tgcd
```

---

## 十、配置文件参考

配置文件路径：`~/.config/tg/config.toml`

```toml
# Telegram API 凭证
[telegram]
api_id = 12345678
api_hash = "abcdef1234567890abcdef1234567890"
phone = "+8613800138000"        # 可选，登录时也可输入

# TDLib 设置
[tdlib]
database_directory = "~/.local/share/tg/tdlib/db"
files_directory = "~/.local/share/tg/tdlib/files"
use_message_database = true     # 使用消息数据库（推荐）
use_secret_chats = false        # 秘密聊天
system_language_code = "zh"     # 语言
device_model = "Telegram-TUI"   # 设备名
verbosity = 0                   # 日志级别 (0=静默, 1=警告, 2=信息, 3+=调试)
test = false                    # 使用测试服务器

# 代理
[proxy]
enabled = false
kind = "socks5"
host = "127.0.0.1"
port = 7890
username = ""
password = ""

# TUI 设置
[tui]
enable_mouse = true             # 鼠标支持
message_page_size = 50          # 每页消息数

# IPC 设置
[ipc]
socket_path = "/run/user/1000/tg/tgcd.sock"  # Socket 路径
```

**也可以用环境变量指定配置文件路径：**

```bash
TG_CONFIG=/path/to/config.toml tg chats
```

---

## 十一、运维管理

### systemd 常用命令

```bash
# 启动
systemctl --user start tgcd

# 停止
systemctl --user stop tgcd

# 重启
systemctl --user restart tgcd

# 查看状态
systemctl --user status tgcd

# 查看实时日志
journalctl --user -u tgcd -f

# 查看最近 100 行日志
journalctl --user -u tgcd -n 100

# 开机自启
systemctl --user enable tgcd

# 取消开机自启
systemctl --user disable tgcd
```

### 手动管理

```bash
# 启动 Daemon
tgcd &

# 停止 Daemon
tg stop

# 带调试日志启动
RUST_LOG=debug tgcd

# 指定配置文件启动
tgcd -c /path/to/config.toml
```

### 更新

```bash
cd Telegram-TUI
git pull
./scripts/install.sh
systemctl --user restart tgcd
```

---

## 十二、目录结构

```text
~/.config/tg/
└── config.toml                 ← 配置文件

~/.local/share/tg/
├── tdlib/
│   ├── db/                     ← TDLib 数据库（session、加密 key、聊天记录）
│   └── files/                  ← TDLib 下载的文件
└── tg.db                       ← SQLite 消息缓存

~/.cache/tg/                    ← 临时缓存

/run/user/$UID/tg/
├── tgcd.sock                   ← IPC Unix Socket
└── tgcd.pid                    ← PID 文件（用于 tg stop）

~/.local/share/keyring/         ← 系统钥匙串（存储 api_hash）
```

---

## 十三、卸载

```bash
./scripts/uninstall.sh
```

脚本会：

1. 停止并禁用 systemd 服务
2. 删除 `tg` 和 `tgcd` 二进制
3. 删除 systemd 服务文件
4. 可选：删除配置和数据

**手动卸载：**

```bash
# 停止 Daemon
systemctl --user stop tgcd
systemctl --user disable tgcd

# 删除二进制
rm -f ~/.cargo/bin/tg ~/.cargo/bin/tgcd

# 删除 systemd 服务
rm -f ~/.config/systemd/user/tgcd.service
systemctl --user daemon-reload

# 删除配置和数据（可选）
rm -rf ~/.config/tg
rm -rf ~/.local/share/tg
rm -rf ~/.cache/tg
```

---

## 十四、常见问题

### Q: `tg status` 报 "Daemon not running"

**原因：** tgcd 没有启动，或 socket 文件不存在。

```bash
# 检查 Daemon 是否在运行
systemctl --user status tgcd

# 如果没有运行，启动它
systemctl --user start tgcd
```

### Q: `tg login` 卡住不动

**原因：** Daemon 可能已经登录了（有已保存的 session），或者网络不通。

```bash
# 查看 Daemon 日志
journalctl --user -u tgcd -f

# 如果 session 损坏，删除后重新登录
systemctl --user stop tgcd
rm -rf ~/.local/share/tg/tdlib
systemctl --user start tgcd
tg login
```

### Q: 编译时找不到 `libtdjson.so`

**原因：** TDLib 没有安装，或安装路径不在 linker 搜索路径中。

```bash
# 方法一：设置环境变量指向 libtdjson 的位置
export LIBTDJSON_PATH=/path/to/libtdjson.so

# 方法二：设置 LD_LIBRARY_PATH
export LD_LIBRARY_PATH=/usr/local/lib:$LD_LIBRARY_PATH

# 方法三：用安装脚本自动处理
BUILD_TDLIB=1 ./scripts/install.sh
```

### Q: 代理不生效

**原因：** 配置修改后没有重启 Daemon。

```bash
# 修改配置后必须重启
systemctl --user restart tgcd

# 查看日志确认代理是否生效
journalctl --user -u tgcd -f
# 应该能看到 "Proxy configured: socks5://127.0.0.1:7890"
```

### Q: TUI 界面乱码

**原因：** 终端不支持 Unicode 或字体缺少字符。

- 确保终端支持 UTF-8
- 使用 Nerd Font 或支持 Emoji 的字体
- 推荐终端：Alacritty / Kitty / WezTerm / iTerm2

### Q: 如何在多台机器上使用同一个账号？

每台机器独立安装 Telegram-TUI，分别 `tg login`。Telegram 允许同一账号在多个设备上同时登录。

### Q: 如何切换账号？

```bash
# 登出当前账号
tg logout

# 重新登录（可以输入不同手机号）
tg login
```

### Q: 消息搜索不工作

Telegram 的搜索通过 TDLib 服务端执行，需要 TDLib 完成初始同步后才能使用。首次登录后等待几分钟让同步完成。

### Q: 如何查看 Daemon 的 TDLib 版本？

```bash
# 通过 pkg-config
pkg-config --modversion tdjson

# 或查看动态库
ls -la /usr/lib/libtdjson*
```
