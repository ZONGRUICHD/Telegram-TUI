# Telegram-TUI 交接文档

更新时间：2026-09-06。改造分支：`feat/telegram-tui`。

## 用户目标

仓库更名为 Telegram-TUI，保留 `tg` CLI 和 `tgcd` daemon；TUI 日常操作参考 Telegram Desktop，视觉采用 Claude Code / OpenCode 一类终端应用的简洁风格；CLI 面向 AI，具备可靠的结构化输出、读取与发送能力；普通用户以手机号、验证码、两步验证密码登录；每一步以中文提交到 GitHub。

## 当前审计

- 架构为 Rust workspace + TDLib JSON + 本地 IPC + ratatui。
- 前端没有 JSON 输出，RPC 失败退出码仍为 0，`--config` 被忽略。
- 登录使用无响应请求，验证码错误不能可靠反馈；密码明文回显。
- TUI 没有请求关联，切换聊天时旧响应可能覆盖新聊天；实时事件名称不匹配，没有有效翻页、草稿或消息操作。
- 标记已读使用空消息 ID，历史请求超过 TDLib 单页上限。
- IPC 无条件引用 Unix socket，Windows 构建被允许失败。
- Release 固定 TDLib 1.8.0，当前代码与新旧协议混用；部分安装地址指向错误的仓库所有者。

## 分步实施

1. 仓库名称与地址、交接文档基线。
2. 跨平台运行基础、认证状态机和应用凭据注入。
3. AI CLI 契约与聊天操作，补足后端的正确性。
4. TUI 布局、交互、实时消息及登录界面。
5. 自动检查、使用说明、功能覆盖表和剩余事项。

每步完成后 commit + push，同一中文 PR 持续更新。不自动合并。

## 登录的技术边界

手机号是用户身份认证方式，`api_id/api_hash` 是客户端应用身份；两者不能互相替代。第三方客户端必须拥有应用凭据。正式构建应由维护者提供凭据，使使用者无需申请或输入 API。开发者源码构建保留配置/环境变量入口，不挪用官方客户端的应用身份。

参考资料：

- [TDLib 入门与授权状态机](https://core.telegram.org/tdlib/getting-started)
- [Telegram 应用注册](https://core.telegram.org/api/obtaining_api_id)
- [Desktop 手机号登录源码](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/intro/intro_phone.cpp)
- [TDLib 官方 API schema](https://github.com/tdlib/td/blob/master/td/generate/scheme/td_api.tl)

参考行为和协议，独立实现 Rust 代码，不复制 Desktop 源文件。

## 兼容约定

- 可执行文件仍为 `tg`、`tgcd`。
- 保留原配置和数据目录标识，避免更名导致账号会话丢失。
- 现有系统包名称暂时保留 `telegram-cli`，仓库显示名为 Telegram-TUI。
- 本地工作目录仍叫 Telegram-CLI，Git origin 已指向新仓库；不移动当前使用中的目录。

## 验证记录

- 第一步：确认工作树初始干净、GitHub 管理权限；仓库更名成功，更新 origin 和安装链接。
- 尚未进行真实账号登录或发送；不把模拟测试当作 Telegram 服务端验收。

### 第二步：运行基础与登录

- Windows 改用仅对象所有者可访问的本地 named pipe；Unix 继续使用 0600 socket。
- TDLib 改为运行时加载，编译、CLI 和模拟测试不依赖本机安装 TDLib。`LIBTDJSON_PATH` 指向动态库完整路径。
- daemon 对 IPC 和 TDLib 数据目录加独占锁，防止重复启动；退出改为 IPC 应答后关闭。
- `--config` 传入初始化/登录，登录会自动启动相邻目录的 `tgcd`。
- 密码/验证码隐藏输入，服务端错误通过匹配请求返回，支持手机号、验证码、两步验证、邮箱验证、重发；未注册号码和服务端要求官方购买的状态给出明确提示。
- 凭据优先级：运行时 `TG_API_ID/TG_API_HASH` → 配置文件 → 构建时 `TG_APP_API_ID/TG_APP_API_HASH`；成对取值，避免错配。未发现仓库已有 Actions secrets，尚不能生成免配置的正式登录包。
- 本机默认 MSVC 缺少构建工具，安装了独立的 `stable-x86_64-pc-windows-gnu` 工具链进行验证，未修改用户默认工具链。
- 官方协议基准：TDLib `d1085f9cebc5a62379991ae1652673954f229c1f`；Desktop 参考提交 `80158983dba09d3bf5d96701f21473d6c34bf5f5`。后续发布应固定同一 TDLib 提交，不能沿用旧版 1.8.0。
- 第二步验证：Windows GNU cargo check、全 workspace 严格 Clippy 通过；IPC 本地往返与请求关联集成测试共 2 项通过。

### 第三步：AI CLI 与消息操作

- JSON 信封、管道默认 JSON、非零错误码、请求超时、dry-run、stdin / 文件输入；详见 [AI CLI 契约](AI-CLI.md)。
- 支持历史和搜索游标、聊天列表分页/文件夹、用户名与收藏夹解析、回复/编辑/撤回/转发、附件发送/下载、置顶/归档/静音、联系人/成员/话题、NDJSON 更新订阅、高级 TDLib 调用。
- 标记已读先获取实际消息 ID；不再对读取操作隐式已读，也不把异步发送受理显示成已送达。
- 官方 schema 新版附件采用 inputDocument / inputPhoto 嵌套，代理采用 proxy 对象；按固定提交实现。
- 第二步 GitHub CI 三个平台全绿。第三步本机全 workspace 测试 13 项通过，严格 Clippy 通过；集成测试使用独立临时配置和模拟 IPC，不使用真实 Telegram 账号。
