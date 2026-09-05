# AI CLI 使用契约

`tg schema` 返回机器可读的命令目录与输出约定；`tg --help` / `tg <command> --help` 查看参数。默认在管道中输出 JSON，交互终端中输出可读文本。`--json` / `--human` 可显式选择。

```json
{"schema_version":1,"ok":true,"result":{"messages":[],"next_from_message_id":null}}
```

失败为 `{"schema_version":1,"ok":false,"error":{"kind":"rpc","code":429,"message":"..."}}`。标准输出只承载结果，诊断写入标准错误；退出码 0 成功、1 操作失败、2 参数错误、3 认证失败（401）、4 超时。保留 Telegram 错误码（如 400、401、403、429），不要解析带表情的自然语言来判断成功。

## 读取

```sh
tg --json status
tg --json me
tg --json chats --limit 30 --list main
tg --json chats --list archive
tg --json folders
tg --json chats --list 2
tg --json find "项目群"
tg --json chat @username
tg --json history -100123 --limit 50
tg --json history -100123 --limit 50 --before 123456789
tg --json search -100123 "关键词" --before 123456789
tg --json message -100123 123456789
tg --json members -100123 --limit 50 --offset 0
tg --json topics -100123
```

聊天参数支持数字 ID（包括负数）、`@username`、`me` / `self`。不按模糊显示名发送，以免同名联系人误发。

历史按消息 ID 倒序返回，下一页使用 `result.next_from_message_id` 作为 `--before`，游标为排他边界；空页游标为 null。TDLib 单页可能少于请求数量，不能以“少于 limit”认定全部读完。聊天列表使用 `result.next_offset`；聊天排序随实时活动变化，因此 offset 翻页期间应按聊天 ID 去重。读取、搜索和订阅不发送已读回执。

## 写入与发送状态

```sh
tg --json --dry-run send me "测试计划"
tg --json send me "Hello"
tg --json send -100123 --reply-to 123456789 "回复"
tg --json send -100123 --topic 42 --silent "话题内消息"
tg --json send me --file message.txt
tg --json send me --stdin
tg --json edit me 123456789 "新文本"
tg --json send-file me ./report.pdf --caption "报告"
tg --json send-file me ./photo.jpg --photo
tg --json forward -100123 me 123456789
tg --json delete me 123456789
tg --json delete me 123456789 --revoke
tg --json read -100123 123456789
tg --json pin -100123
tg --json archive -100123 --undo
tg --json mute -100123 --seconds 0
```

`send --stdin` 保留原始换行和空格；消息最长 4096 个 UTF-16 单元。附件路径在 CLI 端转为绝对路径。`delete` 默认只删除自己一侧，`--revoke` 才请求双方撤回；服务端仍按权限决定是否允许。

成功调用 `send` 仅表示 TDLib 受理。`result.sending_state` 可能仍为 pending：最终 ID 由 `updateMessageSendSucceeded` 提供；失败通过 `updateMessageSendFailed` 返回。请先开启 watch，再发送；超时或连接中断后核对历史，不自动重复发送。`--dry-run` 只检查本地参数并输出 RPC，不解析远端用户名、不连接服务，也不证明服务端权限。

`download <file-id>` 发起异步下载，`file <file-id>` 查看状态，`download <file-id> --wait --timeout 300` 等待完成。

## 事件流

```sh
tg watch --chat -100123 --timeout 300
tg watch --count 10 --timeout 60
```

输出 NDJSON：每行一个 `{"schema_version":1,"type":"event","name":"updateNewMessage","data":{...}}`。保留官方 TDLib 更新名称。`--timeout` 是流的总时长，默认 35 秒；超时结束为 0。连接中断为非零退出；`resync_required` 表示接收落后丢失更新，调用方须重新读取聊天/历史后恢复订阅。此事件流没有持久化重放游标，不能作为唯一消息存档。

## 认证与配置

普通用户运行 `tg login`，已配置应用身份的构建会自动启动服务；会话保存在本机。AI 可以先调用 `tg auth status`，再按状态调用 `tg auth phone/code/password/email/email-code --stdin`。`auth resend` 重新发送，`auth qr` 请求设备确认链接。敏感值不要写进命令参数或日志。

`--config <path>` 对所有入口生效，`TG_CONFIG` 提供默认路径。脚本默认不自动启动服务，需启用 `--start-daemon`；`tg stop` 通过本地 IPC 关闭服务。`tg doctor` 仅报告应用身份是否可用，不输出凭据。

高级操作使用 `tg api <TDLib方法> --params '<JSON>'` 或 `--stdin`，保留官方响应。例如 `tg --json api getUser --params '{"user_id":123}'`。初始化、销毁和日志控制由 daemon 管理，不允许透传覆盖；其余操作依照账号权限执行。协议以交接文档中固定的 TDLib 提交为基准。
