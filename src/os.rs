use anyhow::anyhow;
use tokio::process;

pub async fn hostname() -> anyhow::Result<String> {
    // TODO Consider a cross-platofrm way to lookup hostname.
    let bytes = cmd("hostname", &[]).await?;
    let str = String::from_utf8(bytes)?;
    let str = str.trim();
    Ok(str.to_string())
}

pub async fn cmd(exe: &str, args: &[&str]) -> anyhow::Result<Vec<u8>> {
    let out = process::Command::new(exe).args(args).output().await?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(anyhow!(
            "Failed to execute command: exe={exe:?} args={args:?} err={:?}",
            String::from_utf8_lossy(&out.stderr[..])
        ))
    }
}
