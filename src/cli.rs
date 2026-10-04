use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "brute",
    version,
    about = "Concurrent login brute-force tool (FTP / SSH / HTTP) — for authorized testing only",
    long_about = "A hydra-style credential brute-force tool with FTP, SSH, and HTTP modules.\n\n\
                  Run with no arguments to launch the interactive terminal wizard, or use a \
                  subcommand (ftp/ssh/http) for non-interactive scripting.\n\n\
                  Use ONLY against systems you own or are explicitly authorized to test."
)]
pub struct Cli {
    /// Number of concurrent workers
    #[arg(short = 't', long, global = true, default_value_t = 16)]
    pub threads: usize,

    /// Timeout (seconds) for each connection/attempt
    #[arg(long, global = true, default_value_t = 10)]
    pub timeout: u64,

    /// Delay (milliseconds) after each attempt per worker
    #[arg(long, global = true, default_value_t = 0)]
    pub delay_ms: u64,

    /// Stop after first successful credential
    #[arg(long, global = true, default_value_t = false)]
    pub stop_on_success: bool,

    /// Verbose logging
    #[arg(short = 'v', long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// FTP login brute force
    Ftp {
        /// Target host (IP or hostname)
        host: String,
        /// Port
        #[arg(short = 'P', long, default_value_t = 21)]
        port: u16,
        /// File containing usernames (one per line)
        #[arg(short = 'U', long)]
        users: String,
        /// File containing passwords (one per line)
        #[arg(short = 'W', long)]
        passwords: String,
        /// Single username (overrides users file)
        #[arg(long, conflicts_with = "users")]
        user: Option<String>,
        /// Single password (overrides passwords file)
        #[arg(long, conflicts_with = "passwords")]
        password: Option<String>,
    },
    /// SSH login brute force
    Ssh {
        /// Target host (IP or hostname)
        host: String,
        /// Port
        #[arg(short = 'P', long, default_value_t = 22)]
        port: u16,
        /// File containing usernames (one per line)
        #[arg(short = 'U', long)]
        users: String,
        /// File containing passwords (one per line)
        #[arg(short = 'W', long)]
        passwords: String,
        /// Single username (overrides users file)
        #[arg(long, conflicts_with = "users")]
        user: Option<String>,
        /// Single password (overrides passwords file)
        #[arg(long, conflicts_with = "passwords")]
        password: Option<String>,
    },
    /// HTTP login brute force
    Http {
        /// Full target URL (e.g. https://host/login)
        url: String,
        /// Username(s): single user or path to a user wordlist
        #[arg(short = 'U', long)]
        users: String,
        /// Password(s): single password or path to a password wordlist
        #[arg(short = 'W', long)]
        passwords: String,
        /// Authentication method
        #[arg(long, value_enum, default_value_t = HttpAuth::Basic)]
        auth: HttpAuth,
        /// Username field name (form auth)
        #[arg(long, default_value = "username")]
        user_field: String,
        /// Password field name (form auth)
        #[arg(long, default_value = "password")]
        password_field: String,
        /// Additional static POST body fields as k=v pairs (repeatable)
        #[arg(long, value_parser = parse_kv)]
        data: Vec<(String, String)>,
        /// Text that indicates a successful login (form auth)
        #[arg(long)]
        success_text: Option<String>,
        /// HTTP status code that indicates success (default 200)
        #[arg(long, default_value_t = 200)]
        success_code: u16,
        /// Text that indicates a failed login (form auth)
        #[arg(long)]
        fail_pattern: Option<String>,
        /// Follow redirects
        #[arg(long, default_value_t = true)]
        follow_redirects: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum, PartialEq, Eq, Debug)]
pub enum HttpAuth {
    Basic,
    Form,
}

fn parse_kv(s: &str) -> Result<(String, String), String> {
    let mut parts = s.splitn(2, '=');
    let k = parts.next().unwrap_or("").trim().to_string();
    let v = parts.next().unwrap_or("").trim().to_string();
    if k.is_empty() {
        return Err(format!("invalid k=v pair: {s}"));
    }
    Ok((k, v))
}