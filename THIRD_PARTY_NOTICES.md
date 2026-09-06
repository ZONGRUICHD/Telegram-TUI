# 第三方组件与源码参考

本项目 Rust 代码采用 MIT，见 LICENSE。Telegram-TUI 是第三方客户端，与 Telegram 官方无隶属关系。

## TDLib

- 官方仓库：https://github.com/tdlib/td
- 许可证：Boost Software License 1.0；发行包附带 TDLib-LICENSE.txt。
- 本项目固定的源码版本见 TDLIB_COMMIT，打包后实际版本见 TDLIB-BUILD.json。
- 协议实现由 TDLib 提供，本项目通过 JSON 接口调用。Linux 构建使用官方 SplitSource.php 降低内存需求；scripts/tdlib-split-fix.patch 修正该提交脚本中缺失的逗号，不改动协议或运行逻辑。发行包保留此补丁和来源说明。

## 原生库依赖

TDLib 使用 OpenSSL（3.x 为 Apache-2.0）、zlib（Zlib license）等组件。Windows 打包 vcpkg 构建的运行库及对应版权文件；macOS 链接 Homebrew 的 OpenSSL 静态库并附带许可证；Linux 使用发行版提供的 OpenSSL/zlib 共享库。

## Rust 依赖

准确版本和来源记录在 Cargo.lock 中。主要组件包括 tokio、serde、clap、ratatui、crossterm 和 libloading。发行构建生成 THIRD-PARTY-LICENSES.html，收录实际依赖的许可证文本。

## Telegram Desktop 源码参考

参考 https://github.com/telegramdesktop/tdesktop 的登录步骤和聊天交互，参考提交为 80158983dba09d3bf5d96701f21473d6c34bf5f5。Desktop 采用 GPL-3.0；本项目没有复制其源文件、界面资源或应用身份。手机号授权和消息协议按 TDLib 官方 schema 独立实现。
