# 构建与发行

版本：0.2.0 开发分支。现有 v0.1.0 Release 属于旧版，不能用来验证本次改造。

## 开发构建

需要 Rust 1.88 或更新稳定版。Rust 编译与模拟测试不需要安装 TDLib。

```sh
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
cargo run -p tg -- tui --demo
```

Windows 推荐 Visual Studio 2022/2025 C++ Build Tools 和 Windows SDK。也可安装独立 GNU 工具链并配合 MSYS2 UCRT64 GCC：

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
cargo +stable-x86_64-pc-windows-gnu build --workspace
```

## TDLib 原生库

必须使用根目录 TDLIB_COMMIT 指定的提交。历史的 1.8.0 库不能匹配当前消息附件、代理和授权结构。

Linux/macOS 在安装 CMake、C++ 编译器、gperf、OpenSSL、zlib 后运行 `bash scripts/build-tdlib.sh`，库输出到项目 native 目录。Windows 的完整 CMake/vcpkg 构建方式见 `.github/workflows/release.yml`，该流程也是发行包的构建入口。

库查找顺序：`LIBTDJSON_PATH` 指定的完整文件路径、程序旁边的库、系统库目录。`tgcd --check-library` 无需账号或网络，输出实际 TDLib 版本和提交。`tg doctor` 检查配置、应用身份、相邻 daemon 和原生库。

## 手机号登录与应用身份

Telegram 要求每个第三方应用提供 api_id/api_hash。维护者在 GitHub Actions secrets 设置 **TG_APP_API_ID**、**TG_APP_API_HASH**，编译时嵌入应用身份；普通使用者只需手机号、验证码及按账号要求输入的两步验证密码。不要借用官方客户端的应用身份，也不要把值写入 Git。

源码调试可通过运行时 TG_API_ID/TG_API_HASH 成对配置，或保留旧版配置文件中的应用身份。`tg doctor` 只报告是否存在，不显示内容。应用身份会存在于发行二进制中，这是客户端协议的性质；Actions secrets 防止它误入源码和日志。

## GitHub Actions

- CI 在 Windows、Linux、macOS 执行格式、测试和严格 Clippy。
- `构建发行包` 可手动对分支运行：获取固定 TDLib 源码，构建库和 Rust 程序，打包后实际运行 `tgcd --check-library` 并比对提交，再渲染离线 TUI。
- 产物为 Windows x64 ZIP、Linux x64 tar.gz / deb、macOS arm64 tar.gz。手动开发构建允许未提供应用身份，这种包无法直接完成账号登录。
- v* 标签构建必须有应用身份，并只创建 **草稿 Release**。完成人工账号验收后再发布。
- RPM、Arch、Termux 目录保留为社区源码配方，不列为本次经过发行验证的平台。

Linux 包基于 Ubuntu 22.04，依赖系统 OpenSSL 3/zlib；Windows 使用 MSVC，需 Visual C++ 2015–2022 x64 运行库；macOS 包未签名、未公证。解压时保留可执行文件与原生库的相对位置。

## 安装

发行包可解压到单独目录直接运行 `tg`。源码安装使用 `scripts/install.sh` 或 `scripts/install.ps1`，将两个程序及 native 中的库复制到指定目录，不修改系统服务或自动启动账号。安装前需自行完成 TDLib 构建或从本项目 Actions 下载同版本开发包。

## 发布前必须人工验收

使用维护者自己的测试账号完成手机号／验证码／2FA、退出重进会话、两个客户端之间的收发／回复／编辑／撤回、附件、未读数、断网重连和代理测试。自动测试使用模拟 IPC 和虚构聊天，不能证明 Telegram 服务端行为已经通过。
