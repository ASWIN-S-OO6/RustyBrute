use anyhow::{Context, Result};
use russh::keys::PublicKeyOrCertificate;
use std::sync::Arc;
use tokio::time::{timeout, Duration};

/// Minimal client handler that accepts any server host key (typical for
/// brute-force tooling where trust-on-first-use is not applicable).
#[derive(Clone)]
struct SshHandler;

impl russh::client::Handler for SshHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Attempt an SSH password login. Returns `Ok(Some(msg))` on success.
pub async fn try_login(
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
    timeout_secs: u64,
) -> Result<Option<String>> {
    let deadline = Duration::from_secs(timeout_secs);
    let config = Arc::new(russh::client::Config::default());
    let addr = format!("{host}:{port}");

    let mut client: russh::client::Handle<SshHandler> =
        timeout(deadline, russh::client::connect(config, addr, SshHandler))
            .await
            .with_context(|| format!("SSH connect to {host}:{port}"))??;

    let auth_result = timeout(
        deadline,
        client.authenticate_password(user, pass.to_string()),
    )
    .await;

    // Always try to disconnect cleanly before evaluating the result.
    let _ = timeout(
        deadline,
        client.disconnect(russh::Disconnect::ByApplication, "", ""),
    )
    .await;

    match auth_result {
        Ok(Ok(russh::client::AuthResult::Success)) => {
            Ok(Some(format!("SSH {user}:{pass} @ {host}:{port} (auth OK)")))
        }
        Ok(Ok(russh::client::AuthResult::Failure { .. })) => Ok(None),
        Ok(Err(e)) => Err(e).with_context(|| format!("SSH auth error for {user}")),
        Err(_) => anyhow::bail!("SSH auth timed out for {user}"),
    }
}