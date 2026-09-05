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
