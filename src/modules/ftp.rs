use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

/// Attempt an FTP login. Returns `Ok(Some(msg))` on a successful login (230),
/// `Ok(None)` on a failed login, and an error on network/protocol issues.
pub async fn try_login(
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
    timeout_secs: u64,
) -> Result<Option<String>> {
    let deadline = Duration::from_secs(timeout_secs);
    let stream = timeout(deadline, TcpStream::connect((host, port)))
        .await
        .with_context(|| format!("connect to {host}:{port}"))??;
    stream.set_nodelay(true).ok();

    let (read_half, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Read the server greeting (220).
    let _greeting = read_reply(&mut reader, deadline).await?;

    let user_cmd = format!("USER {user}\r\n");
    timeout(deadline, writer.write_all(user_cmd.as_bytes())).await??;
    timeout(deadline, writer.flush()).await??;
    let user_reply = read_reply(&mut reader, deadline).await?;

    // A 331 means password is required; 230 may mean no password needed.
    if !user_reply.starts_with("331") && !user_reply.starts_with("230") {
        return Ok(None);
    }

    let pass_cmd = format!("PASS {pass}\r\n");
    timeout(deadline, writer.write_all(pass_cmd.as_bytes())).await??;
    timeout(deadline, writer.flush()).await??;
    let pass_reply = read_reply(&mut reader, deadline).await?;

    // Try to close cleanly so we don't spam connections.
    let _ = timeout(deadline, writer.write_all(b"QUIT\r\n")).await;
    let _ = timeout(deadline, writer.flush()).await;

    if pass_reply.starts_with("230") {
        Ok(Some(format!("FTP {user}:{pass} @ {host}:{port} (230 OK)")))
    } else {
        Ok(None)
    }
}

async fn read_reply<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    deadline: Duration,
) -> Result<String> {
    let mut line = String::new();
    timeout(deadline, reader.read_line(&mut line)).await??;
    if line.is_empty() {
        anyhow::bail!("connection closed by server");
    }
    Ok(line.trim_end().to_string())
}