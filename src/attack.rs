use crate::cli::HttpAuth;
use crate::modules;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    Ftp,
    Ssh,
    Http,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ftp => "FTP",
            Self::Ssh => "SSH",
            Self::Http => "HTTP",
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Ftp => 21,
            Self::Ssh => 22,
            Self::Http => 80,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Ftp => "File Transfer Protocol (default port 21)",
            Self::Ssh => "Secure Shell (default port 22)",
            Self::Http => "Web login — Basic or form auth",
        }
    }
}

/// HTTP-specific options, only used when `Method::Http` is selected.
#[derive(Clone, Debug)]
pub struct HttpOptions {
    pub auth: HttpAuth,
    pub user_field: String,
    pub password_field: String,
    pub extra: Vec<(String, String)>,
    pub success_text: Option<String>,
    pub fail_pattern: Option<String>,
    pub success_code: u16,
    pub follow_redirects: bool,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            auth: HttpAuth::Basic,
            user_field: "username".into(),
            password_field: "password".into(),
            extra: vec![],
            success_text: None,
            fail_pattern: None,
            success_code: 200,
            follow_redirects: true,
        }
    }
}

/// A fully resolved, ready-to-run attack description.
#[derive(Clone, Debug)]
pub struct AttackConfig {
    pub method: Method,
    /// Host for FTP/SSH; full URL for HTTP.
    pub host: String,
    pub port: u16,
    pub users: Vec<String>,
    pub passwords: Vec<String>,
    pub threads: usize,
    pub timeout: u64,
    pub delay_ms: u64,
    pub stop_on_success: bool,
    pub http: Option<HttpOptions>,
}

impl AttackConfig {
    pub fn total(&self) -> usize {
        self.users.len().saturating_mul(self.passwords.len())
    }

    pub fn target_label(&self) -> String {
        match self.method {
            Method::Http => format!("HTTP {}", self.host),
            m => format!("{} {}:{}", m.label(), self.host, self.port),
        }
    }
}

/// Live events emitted while an attack is running. Consumed by the TUI
/// (progress gauge, found-credential list) and by direct CLI mode (logging).
#[derive(Clone, Debug)]
pub enum AttackEvent {
    Started { total: usize },
    AttemptDone {
        done: usize,
        user: String,
        pass: String,
        success: bool,
        detail: String,
    },
    Finished {
        attempted: usize,
        found: usize,
        cancelled: bool,
    },
}

/// Run a full brute-force attack, streaming events into `tx` until done.
///
/// The `cancel` flag aborts the run early (used by Ctrl+C / Esc in the TUI).
/// This is the single execution path shared by both the interactive wizard
/// and the non-interactive subcommand mode.
pub async fn run_attack(
    cfg: AttackConfig,
    tx: mpsc::UnboundedSender<AttackEvent>,
    cancel: Arc<AtomicBool>,
) {
    let total = cfg.total();
    let _ = tx.send(AttackEvent::Started { total });

    let (combo_tx, combo_rx) = mpsc::channel::<(String, String)>(1024);
    {
        let users = cfg.users.clone();
        let passwords = cfg.passwords.clone();
        tokio::spawn(async move {
            for u in &users {
                for p in &passwords {
                    if combo_tx.send((u.clone(), p.clone())).await.is_err() {
                        return; // all workers exited
                    }
                }
            }
        });
    }

    let rx = Arc::new(Mutex::new(combo_rx));
    let done = Arc::new(AtomicUsize::new(0));
    let found = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..cfg.threads.max(1) {
        let rx = rx.clone();
        let tx = tx.clone();
        let cancel = cancel.clone();
        let done = done.clone();
        let found = found.clone();
        let cfg = cfg.clone();
        handles.push(tokio::spawn(async move {
            loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if cfg.stop_on_success && found.load(Ordering::Relaxed) > 0 {
                    break;
                }
                let item = {
                    let mut guard = rx.lock().await;
                    guard.recv().await
                };
                let Some((user, pass)) = item else { break };

                let res = attempt(&cfg, &user, &pass).await;
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                let (success, detail) = match res {
                    Ok(Some(msg)) => {
                        found.fetch_add(1, Ordering::Relaxed);
                        (true, msg)
                    }
                    Ok(None) => (false, String::new()),
                    Err(e) => (false, format!("{e:#}")),
                };
                let _ = tx.send(AttackEvent::AttemptDone {
                    done: d,
                    user,
                    pass,
                    success,
                    detail,
                });

                if cfg.delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(cfg.delay_ms)).await;
                }
            }
        }));
    }

    for h in handles {
        let _ = h.await;
    }

    let attempted = done.load(Ordering::Relaxed);
    let found = found.load(Ordering::Relaxed);
    let _ = tx.send(AttackEvent::Finished {
        attempted,
        found,
        cancelled: cancel.load(Ordering::Relaxed),
    });
}

async fn attempt(cfg: &AttackConfig, user: &str, pass: &str) -> Result<Option<String>> {
    match cfg.method {
        Method::Ftp => modules::ftp::try_login(&cfg.host, cfg.port, user, pass, cfg.timeout).await,
        Method::Ssh => modules::ssh::try_login(&cfg.host, cfg.port, user, pass, cfg.timeout).await,
        Method::Http => {
            let Some(h) = cfg.http.as_ref() else {
                return Ok(None);
            };
            modules::http::try_login(
                &cfg.host,
                user,
                pass,
                h.auth,
                &h.user_field,
                &h.password_field,
                &h.extra,
                h.success_text.as_deref(),
                h.fail_pattern.as_deref(),
                h.success_code,
                h.follow_redirects,
                cfg.timeout,
            )
            .await
        }
    }
}