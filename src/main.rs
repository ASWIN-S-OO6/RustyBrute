mod attack;
mod cli;
mod modules;
mod tui;
mod wordlist;

use anyhow::Result;
use clap::Parser;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::attack::{run_attack, AttackConfig, AttackEvent, HttpOptions, Method};
use crate::cli::{Cli, Command};
use crate::wordlist::load_words;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Extract the global options up front so `cli.command` can be moved out
    // without partially moving `cli`.
    let globals = Globals {
        threads: cli.threads,
        timeout: cli.timeout,
        delay_ms: cli.delay_ms,
        stop_on_success: cli.stop_on_success,
        verbose: cli.verbose,
    };

    match cli.command {
        None => tui::run().await,
        Some(cmd) => run_direct(globals, cmd).await,
    }
}

#[derive(Clone, Copy)]
struct Globals {
    threads: usize,
    timeout: u64,
    delay_ms: u64,
    stop_on_success: bool,
    verbose: bool,
}

// ----------------------------------------------------------------- direct mode

async fn run_direct(g: Globals, cmd: Command) -> Result<()> {
    let filter = if g.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    env_logger::Builder::new()
        .filter_level(filter)
        .format_timestamp_secs()
        .init();

    let cfg = build_config(g, cmd)?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cancel = Arc::new(AtomicBool::new(false));

    // Ctrl+C triggers a graceful cancel instead of killing the process.
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                log::warn!("cancelling… (Ctrl+C again to force quit)");
                cancel.store(true, Ordering::Relaxed);
            }
        });
    }

    let started = Instant::now();
    let target = cfg.target_label();
    let total = cfg.total();
    log::info!(
        "{target} — {} users × {} passwords = {total} combinations, {} workers",
        cfg.users.len(),
        cfg.passwords.len(),
        cfg.threads,
    );

    let handle = tokio::spawn(run_attack(cfg, tx, cancel));

    let mut last_progress = Instant::now() - Duration::from_secs(3);
    while let Some(ev) = rx.recv().await {
        match ev {
            AttackEvent::Started { .. } => {}
            AttackEvent::AttemptDone {
                done,
                user,
                pass,
                success,
                detail,
            } => {
                if success {
                    log::info!("[+] {detail}");
                } else if g.verbose && !detail.is_empty() {
                    log::debug!("#{done} {user}:{pass} → {detail}");
                }
                if last_progress.elapsed() >= Duration::from_secs(2) {
                    log::info!(
                        "[progress] {done}/{total} ({:.0}%)",
                        100.0 * done as f64 / total.max(1) as f64
                    );
                    last_progress = Instant::now();
                }
            }
            AttackEvent::Finished {
                attempted,
                found,
                cancelled,
            } => {
                if cancelled {
                    log::warn!("run cancelled");
                }
                log::info!(
                    "Finished: {found} valid credential(s), {attempted} attempts in {:.1}s",
                    started.elapsed().as_secs_f64()
                );
                break;
            }
        }
    }

    handle.await?;
    Ok(())
}

fn build_config(g: Globals, cmd: Command) -> Result<AttackConfig> {
    let (method, host, port, users, passwords, http) = match cmd {
        Command::Ftp {
            host,
            port,
            users,
            passwords,
            user,
            password,
        } => {
            let users = match user {
                Some(u) => vec![u],
                None => load_words(&users)?,
            };
            let passwords = match password {
                Some(p) => vec![p],
                None => load_words(&passwords)?,
            };
            (Method::Ftp, host, port, users, passwords, None)
        }
        Command::Ssh {
            host,
            port,
            users,
            passwords,
            user,
            password,
        } => {
            let users = match user {
                Some(u) => vec![u],
                None => load_words(&users)?,
            };
            let passwords = match password {
                Some(p) => vec![p],
                None => load_words(&passwords)?,
            };
            (Method::Ssh, host, port, users, passwords, None)
        }
        Command::Http {
            url,
            users,
            passwords,
            auth,
            user_field,
            password_field,
            data,
            success_text,
            success_code,
            fail_pattern,
            follow_redirects,
        } => {
            let users = wordlist::resolve_words(&users)?;
            let passwords = wordlist::resolve_words(&passwords)?;
            let http = HttpOptions {
                auth,
                user_field,
                password_field,
                extra: data,
                success_text,
                fail_pattern,
                success_code,
                follow_redirects,
            };
            (Method::Http, url, 0, users, passwords, Some(http))
        }
    };

    Ok(AttackConfig {
        method,
        host,
        port,
        users,
        passwords,
        threads: g.threads.max(1),
        timeout: g.timeout.max(1),
        delay_ms: g.delay_ms,
        stop_on_success: g.stop_on_success,
        http,
    })
}