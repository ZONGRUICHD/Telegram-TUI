use anyhow::Result;
use std::path::Path;
use tg_core::config::TgConfig;

pub fn run(path: &Path) -> Result<()> {
    if path.exists() {
        TgConfig::load_from(path)?;
        eprintln!("配置已存在：{}", path.display());
        return Ok(());
    }
    TgConfig::default().save_to(path)?;
    eprintln!(
        "配置已创建：{}。运行 tg login 以手机号登录。",
        path.display()
    );
    Ok(())
}
