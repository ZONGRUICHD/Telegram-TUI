//! User authentication, driven by Telegram's current state, with retryable errors.
use anyhow::Result;
use serde_json::{json, Value};
use std::{
    io::{IsTerminal, Write},
    path::Path,
};
use tg_core::config::TgConfig;

pub async fn run(path: &Path) -> Result<()> {
    anyhow::ensure!(
        std::io::stdin().is_terminal(),
        "交互登录需要终端；AI 请使用 tg auth 子命令"
    );
    if !path.exists() {
        TgConfig::default().save_to(path)?;
    }
    let config = TgConfig::load_from(path)?;
    let mut client = crate::runtime::ensure_daemon(&config, path).await?;
    eprintln!("Telegram-TUI · 登录\n手机号 → 验证码 → 两步验证密码");
    loop {
        let auth = client.call("auth_trigger", json!({})).await?;
        let state = auth["@type"].as_str().unwrap_or("");
        let (method, params) = match state {
            "authorizationStateReady" => {
                eprintln!("登录成功，会话已保存。");
                return Ok(());
            }
            "authorizationStateWaitPhoneNumber" => (
                "auth_phone",
                json!({"phone":input("手机号（含国家区号）", false)?}),
            ),
            "authorizationStateWaitCode" => {
                eprintln!("{}", auth_hint(&auth));
                let code = input("验证码（输入 /resend 重新发送）", true)?;
                if code == "/resend" {
                    ("auth_resend", json!({}))
                } else {
                    ("auth_code", json!({"code":code}))
                }
            }
            "authorizationStateWaitPassword" => {
                eprintln!("{}", auth_hint(&auth));
                (
                    "auth_password",
                    json!({"password":input("两步验证密码", true)?}),
                )
            }
            "authorizationStateWaitEmailAddress" => {
                ("auth_email", json!({"email":input("登录邮箱", false)?}))
            }
            "authorizationStateWaitEmailCode" => {
                eprintln!("{}", auth_hint(&auth));
                (
                    "auth_email_code",
                    json!({"code":input("邮箱验证码", true)?}),
                )
            }
            "authorizationStateWaitOtherDeviceConfirmation" => {
                eprintln!(
                    "请在已登录的 Telegram 设备确认：{}",
                    auth["link"].as_str().unwrap_or("")
                );
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
            "authorizationStateWaitRegistration" => {
                anyhow::bail!("此号码尚未注册，请先通过官方 Telegram 注册后重新登录")
            }
            "authorizationStateWaitPremiumPurchase" => anyhow::bail!(
                "Telegram 要求在官方客户端完成登录购买流程，请使用官方客户端处理后重试"
            ),
            "authorizationStateClosed"
            | "authorizationStateClosing"
            | "authorizationStateLoggingOut" => anyhow::bail!("会话已关闭，请重新运行 tg login"),
            other => anyhow::bail!("暂不支持的登录状态：{other}"),
        };
        if let Err(error) = client.call(method, params).await {
            if error.downcast_ref::<tg_ipc::protocol::RpcError>().is_none() {
                return Err(error);
            }
            eprintln!("登录失败：{error}。请重新输入。");
        }
    }
}

pub fn auth_hint(auth: &Value) -> String {
    match auth["@type"].as_str().unwrap_or("") {
        "authorizationStateWaitCode" => {
            let channel = match auth["code_info"]["type"]["@type"].as_str().unwrap_or("") {
                "authenticationCodeTypeTelegramMessage" => "已登录的 Telegram 设备",
                "authenticationCodeTypeSms" => "手机短信",
                "authenticationCodeTypeCall" => "语音电话",
                _ => "Telegram 指定的验证渠道",
            };
            let seconds = auth["code_info"]["timeout"].as_i64().unwrap_or(0);
            format!("请检查{channel}；重新发送等待 {seconds} 秒")
        }
        "authorizationStateWaitPassword" => format!(
            "密码提示：{}",
            auth["password_hint"].as_str().unwrap_or("无")
        ),
        "authorizationStateWaitEmailCode" => format!(
            "验证码已发送到 {}",
            auth["code_info"]["email_address_pattern"]
                .as_str()
                .unwrap_or("登录邮箱")
        ),
        _ => String::new(),
    }
}

pub fn input(prompt: &str, hidden: bool) -> Result<String> {
    if hidden {
        return Ok(rpassword::prompt_password(format!("{prompt}："))?);
    }
    eprint!("{prompt}：");
    std::io::stderr().flush()?;
    let mut value = String::new();
    anyhow::ensure!(
        std::io::stdin().read_line(&mut value)? != 0,
        "输入结束，登录已取消"
    );
    Ok(value.trim().to_owned())
}
