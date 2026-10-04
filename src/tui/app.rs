use crate::attack::{self, AttackConfig, AttackEvent, HttpOptions, Method};
use crate::cli::HttpAuth;
use crate::tui::browser::{Browser, BrowserOutcome};
use crate::wordlist;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Method,
    Target,
    Credentials,
    Options,
    Confirm,
    Running,
    Results,
}

impl Step {
    /// Position within the 5-step wizard (0 for the run/result screens).
    pub fn index(self) -> usize {
        match self {
            Self::Method => 1,
            Self::Target => 2,
            Self::Credentials => 3,
            Self::Options => 4,
            Self::Confirm => 5,
            _ => 0,
        }
    }
}

pub struct FoundCred {
    pub user: String,
    pub pass: String,
    pub detail: String,
}

#[derive(Clone)]
pub struct FinishedInfo {
    pub attempted: usize,
    pub found: usize,
    pub cancelled: bool,
}

pub struct RunView {
    pub total: usize,
    pub done: usize,
    pub found: Vec<FoundCred>,
    pub errors: usize,
    pub started: Instant,
    pub finished: Option<FinishedInfo>,
    pub cancelling: bool,
    pub cancel: Arc<AtomicBool>,
    pub rx: mpsc::UnboundedReceiver<AttackEvent>,
    pub target_label: String,
}

pub struct App {
    pub step: Step,
    pub should_quit: bool,
    pub method: Method,
    pub method_idx: usize,
    pub host: String,
    pub port: String,
    pub users_path: String,
    pub pass_path: String,
    pub users: Option<Vec<String>>,
    pub passwords: Option<Vec<String>>,
    pub users_err: Option<String>,
    pub pass_err: Option<String>,
    pub threads: String,
    pub timeout: String,
    pub delay: String,
    pub stop_on_success: bool,
    pub http_auth: HttpAuth,
    pub user_field: String,
    pub pass_field: String,
    pub success_text: String,
    pub success_code: String,
    pub fail_pattern: String,
    pub follow: bool,
    pub cursor: usize,
    pub msg: Option<(String, Instant, bool)>,
    pub browser: Option<Browser>,
    pub run: Option<RunView>,
}

impl App {
    pub fn new() -> Self {
        Self {
            step: Step::Method,
            should_quit: false,
            method: Method::Ftp,
            method_idx: 0,
            host: String::new(),
            port: String::new(),
            users_path: String::new(),
            pass_path: String::new(),
            users: None,
            passwords: None,
            users_err: None,
            pass_err: None,
            threads: "16".into(),
            timeout: "10".into(),
            delay: "0".into(),
            stop_on_success: true,
            http_auth: HttpAuth::Basic,
            user_field: "username".into(),
            pass_field: "password".into(),
            success_text: String::new(),
            success_code: "200".into(),
            fail_pattern: String::new(),
            follow: true,
            cursor: 0,
            msg: None,
            browser: None,
            run: None,
        }
    }

    // ------------------------------------------------------------------ util

    fn set_err(&mut self, s: impl Into<String>) {
        self.msg = Some((s.into(), Instant::now(), true));
    }

    fn set_info(&mut self, s: impl Into<String>) {
        self.msg = Some((s.into(), Instant::now(), false));
    }

    pub fn is_http(&self) -> bool {
        self.method == Method::Http
    }

    /// Number of interactive rows on the current step (fields + continue).
    pub fn field_count(&self) -> usize {
        match self.step {
            Step::Method => 3,
            Step::Target => {
                if self.is_http() {
                    2
                } else {
                    3
                }
            }
            Step::Credentials => 3,
            Step::Options => {
                if self.is_http() {
                    12
                } else {
                    5
                }
            }
            Step::Confirm => 2,
            _ => 0,
        }
    }

    /// Expire the transient footer message after a few seconds.
    pub fn tick(&mut self) {
        if let Some((_, t, _)) = &self.msg {
            if t.elapsed() > Duration::from_secs(4) {
                self.msg = None;
            }
        }
    }

    pub fn request_quit(&mut self) {
        if let Some(r) = &self.run {
            r.cancel.store(true, Ordering::Relaxed);
        }
        self.should_quit = true;
    }

    // ------------------------------------------------------------- main input

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.request_quit();
            return;
        }

        if self.browser.is_some() {
            let outcome = self.browser.as_mut().map(|b| b.handle_key(&key)).unwrap();
            match outcome {
                BrowserOutcome::None => {}
                BrowserOutcome::Close => self.browser = None,
                BrowserOutcome::Selected(path) => {
                    let for_users = self.browser.as_ref().map(|b| b.for_users).unwrap_or(false);
                    self.browser = None;
                    let p = path.display().to_string();
                    if for_users {
                        self.users_path = p;
                        self.resolve_users();
                    } else {
                        self.pass_path = p;
                        self.resolve_passwords();
                    }
                }
            }
            return;
        }

        match self.step {
            Step::Method => self.on_key_method(key),
            Step::Target => self.on_key_target(key),
            Step::Credentials => self.on_key_creds(key),
            Step::Options => self.on_key_options(key),
            Step::Confirm => self.on_key_confirm(key),
            Step::Running => self.on_key_running(key),
            Step::Results => self.on_key_results(key),
        }
    }

    // --------------------------------------------------------------- steps

    fn on_key_method(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.method_idx = self.method_idx.saturating_sub(1),
            KeyCode::Down => {
                if self.method_idx < 2 {
                    self.method_idx += 1;
                }
            }
            KeyCode::Enter => {
                self.method = match self.method_idx {
                    0 => Method::Ftp,
                    1 => Method::Ssh,
                    _ => Method::Http,
                };
                self.port = self.method.default_port().to_string();
                self.step = Step::Target;
                self.cursor = 0;
            }
            KeyCode::Char('q') | KeyCode::Esc => self.request_quit(),
            _ => {}
        }
    }

    fn on_key_target(&mut self, key: KeyEvent) {
        let fc = self.field_count();
        match key.code {
            KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down => {
                if self.cursor + 1 < fc {
                    self.cursor += 1;
                }
            }
            KeyCode::Esc => {
                self.step = Step::Method;
                self.cursor = 0;
            }
            KeyCode::Backspace => self.edit_target(true),
            KeyCode::Char(c) if !c.is_control() => self.edit_target_char(c),
            KeyCode::Enter => {
                if self.cursor + 1 == fc {
                    if self.validate_target() {
                        self.step = Step::Credentials;
                        self.cursor = 0;
                    }
                } else {
                    self.cursor += 1;
                }
            }
            _ => {}
        }
    }

    fn edit_target(&mut self, backspace: bool) {
        if self.cursor >= self.field_count() - 1 {
            return;
        }
        let field = if self.cursor == 0 {
            Some(&mut self.host)
        } else if !self.is_http() {
            Some(&mut self.port)
        } else {
            None
        };
        if let Some(f) = field {
            if backspace {
                f.pop();
            }
        }
    }

    fn edit_target_char(&mut self, c: char) {
        if self.cursor >= self.field_count() - 1 {
            return;
        }
        let numeric = !self.is_http() && self.cursor == 1;
        if numeric && !c.is_ascii_digit() {
            return;
        }
        if self.cursor == 0 {
            self.host.push(c);
        } else if !self.is_http() {
            self.port.push(c);
        }
    }

    fn validate_target(&mut self) -> bool {
        let host = self.host.trim().to_string();
        if host.is_empty() {
            self.set_err("enter a target host or URL");
            return false;
        }
        if self.is_http() {
            self.host = if host.contains("://") {
                host
            } else {
                format!("http://{host}")
            };
            return true;
        }
        match self.port.trim().parse::<u16>() {
            Ok(p) if p > 0 => {
                self.port = p.to_string();
                true
            }
            _ => {
                self.set_err("port must be a number between 1 and 65535");
                false
            }
        }
    }

    fn on_key_creds(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down => {
                if self.cursor + 1 < 3 {
                    self.cursor += 1;
                }
            }
            KeyCode::Esc => {
                self.step = Step::Target;
                self.cursor = 0;
            }
            KeyCode::Backspace => self.edit_creds(true),
            KeyCode::Char(c) if !c.is_control() => self.edit_creds_char(c),
            KeyCode::Enter => match self.cursor {
                0 | 1 => {
                    self.browser = Some(Browser::new(self.cursor == 0));
                }
                _ => {
                    self.resolve_users();
                    self.resolve_passwords();
                    if self.users.is_some() && self.passwords.is_some() {
                        self.step = Step::Options;
                        self.cursor = 0;
                    } else {
                        self.set_err("fix the highlighted credential fields first");
                    }
                }
            },
            _ => {}
        }
    }

    fn edit_creds(&mut self, backspace: bool) {
        if self.cursor >= 2 {
            return;
        }
        let f = if self.cursor == 0 {
            &mut self.users_path
        } else {
            &mut self.pass_path
        };
        if backspace {
            f.pop();
        }
    }

    fn edit_creds_char(&mut self, c: char) {
        if self.cursor >= 2 {
            return;
        }
        if self.cursor == 0 {
            self.users_path.push(c);
        } else {
            self.pass_path.push(c);
        }
    }

    fn resolve_users(&mut self) {
        if self.users_path.trim().is_empty() {
            self.users = None;
            self.users_err = Some("required".into());
            return;
        }
        match wordlist::resolve_words(&self.users_path) {
            Ok(v) => {
                self.users_err = None;
                self.users = Some(v);
            }
            Err(e) => {
                self.users = None;
                self.users_err = Some(format!("{e:#}"));
            }
        }
    }

    fn resolve_passwords(&mut self) {
        if self.pass_path.trim().is_empty() {
            self.passwords = None;
            self.pass_err = Some("required".into());
            return;
        }
        match wordlist::resolve_words(&self.pass_path) {
            Ok(v) => {
                self.pass_err = None;
                self.passwords = Some(v);
            }
            Err(e) => {
                self.passwords = None;
                self.pass_err = Some(format!("{e:#}"));
            }
        }
    }

    fn on_key_options(&mut self, key: KeyEvent) {
        let fc = self.field_count();
        let http = self.is_http();
        match key.code {
            KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Down => {
                if self.cursor + 1 < fc {
                    self.cursor += 1;
                }
            }
            KeyCode::Esc => {
                self.step = Step::Credentials;
                self.cursor = 0;
            }
            KeyCode::Backspace => self.edit_options(true),
            KeyCode::Char(c) if !c.is_control() => self.edit_options_char(c),
            KeyCode::Left | KeyCode::Right => {
                if http && self.cursor == 4 {
                    self.http_auth = if self.http_auth == HttpAuth::Basic {
                        HttpAuth::Form
                    } else {
                        HttpAuth::Basic
                    };
                }
            }
            KeyCode::Enter => match self.cursor {
                3 => self.stop_on_success = !self.stop_on_success,
                4 if http => {
                    self.http_auth = if self.http_auth == HttpAuth::Basic {
                        HttpAuth::Form
                    } else {
                        HttpAuth::Basic
                    };
                }
                10 if http => self.follow = !self.follow,
                x if x + 1 == fc => {
                    if self.validate_options() {
                        self.step = Step::Confirm;
                        self.cursor = 0;
                    }
                }
                x => self.cursor = (x + 1).min(fc - 1),
            },
            _ => {}
        }
    }

    fn edit_options(&mut self, backspace: bool) {
        let Some(f) = self.options_text_field() else { return };
        if backspace {
            f.pop();
        }
    }

    fn edit_options_char(&mut self, c: char) {
        let numeric = matches!(self.cursor, 0 | 1 | 2 | 8);
        if numeric && !c.is_ascii_digit() {
            return;
        }
        if let Some(f) = self.options_text_field() {
            f.push(c);
        }
    }

    fn options_text_field(&mut self) -> Option<&mut String> {
        let http = self.is_http();
        match self.cursor {
            0 => Some(&mut self.threads),
            1 => Some(&mut self.timeout),
            2 => Some(&mut self.delay),
            5 if http => Some(&mut self.user_field),
            6 if http => Some(&mut self.pass_field),
            7 if http => Some(&mut self.success_text),
            8 if http => Some(&mut self.success_code),
            9 if http => Some(&mut self.fail_pattern),
            _ => None,
        }
    }

    fn validate_options(&mut self) -> bool {
        match self.threads.trim().parse::<usize>() {
            Ok(t) if t >= 1 => {}
            _ => {
                self.set_err("threads must be a number >= 1");
                return false;
            }
        }
        match self.timeout.trim().parse::<u64>() {
            Ok(t) if t >= 1 => {}
            _ => {
                self.set_err("timeout must be a number >= 1 second");
                return false;
            }
        }
        if self.delay.trim().parse::<u64>().is_err() {
            self.set_err("delay must be a number (ms)");
            return false;
        }
        if self.is_http() {
            if self.user_field.trim().is_empty() || self.pass_field.trim().is_empty() {
                self.set_err("HTTP form field names cannot be empty");
                return false;
            }
            if self.success_code.trim().parse::<u16>().is_err() {
                self.set_err("success code must be a valid HTTP status");
                return false;
            }
        }
        true
    }

    fn on_key_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left | KeyCode::Up => self.cursor = 0,
            KeyCode::Right | KeyCode::Down => self.cursor = 1,
            KeyCode::Esc => {
                self.step = Step::Options;
                self.cursor = 0;
            }
            KeyCode::Enter => match self.cursor {
                0 => self.start_attack(),
                _ => {
                    self.step = Step::Options;
                    self.cursor = 0;
                }
            },
            _ => {}
        }
    }

    fn on_key_running(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                let mut info = false;
                if let Some(r) = self.run.as_mut() {
                    if r.finished.is_none() && !r.cancelling {
                        r.cancel.store(true, Ordering::Relaxed);
                        r.cancelling = true;
                        info = true;
                    }
                }
                if info {
                    self.set_info("cancelling…");
                }
            }
            KeyCode::Char('q') => self.request_quit(),
            _ => {}
        }
    }

    fn on_key_results(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left | KeyCode::Up => self.cursor = 0,
            KeyCode::Right | KeyCode::Down => self.cursor = 1,
            KeyCode::Enter => match self.cursor {
                0 => {
                    *self = App::new();
                }
                _ => self.request_quit(),
            },
            KeyCode::Char('q') => self.request_quit(),
            _ => {}
        }
    }

    // ------------------------------------------------------------- attack

    fn start_attack(&mut self) {
        let users = self.users.clone().unwrap_or_default();
        let passwords = self.passwords.clone().unwrap_or_default();
        let port: u16 = if self.is_http() {
            0
        } else {
            self.port
                .trim()
                .parse()
                .unwrap_or(self.method.default_port())
        };
        let http = if self.is_http() {
            Some(HttpOptions {
                auth: self.http_auth,
                user_field: self.user_field.trim().to_string(),
                password_field: self.pass_field.trim().to_string(),
                extra: vec![],
                success_text: none_if_empty(&self.success_text),
                fail_pattern: none_if_empty(&self.fail_pattern),
                success_code: self.success_code.trim().parse().unwrap_or(200),
                follow_redirects: self.follow,
            })
        } else {
            None
        };
        let cfg = AttackConfig {
            method: self.method,
            host: self.host.trim().to_string(),
            port,
            users,
            passwords,
            threads: self.threads.trim().parse().unwrap_or(16).max(1),
            timeout: self.timeout.trim().parse().unwrap_or(10).max(1),
            delay_ms: self.delay.trim().parse().unwrap_or(0),
            stop_on_success: self.stop_on_success,
            http,
        };
        let total = cfg.total();
        let target_label = cfg.target_label();
        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = Arc::new(AtomicBool::new(false));
        tokio::spawn(attack::run_attack(cfg, tx, cancel.clone()));
        self.run = Some(RunView {
            total,
            done: 0,
            found: Vec::new(),
            errors: 0,
            started: Instant::now(),
            finished: None,
            cancelling: false,
            cancel,
            rx,
            target_label,
        });
        self.step = Step::Running;
        self.cursor = 0;
    }

    /// Drain pending attack events into the UI state; must be called
    /// regularly while on the Running/Results screens.
    pub fn poll_attack(&mut self) {
        let Some(run) = self.run.as_mut() else { return };
        while let Ok(ev) = run.rx.try_recv() {
            match ev {
                AttackEvent::Started { total } => run.total = total,
                AttackEvent::AttemptDone {
                    done,
                    user,
                    pass,
                    success,
                    detail,
                } => {
                    run.done = done;
                    if success {
                        run.found.push(FoundCred { user, pass, detail });
                    } else if !detail.is_empty() {
                        run.errors += 1;
                    }
                }
                AttackEvent::Finished {
                    attempted,
                    found,
                    cancelled,
                } => {
                    run.finished = Some(FinishedInfo {
                        attempted,
                        found,
                        cancelled,
                    });
                }
            }
        }
        if self.run.as_ref().is_some_and(|r| r.finished.is_some()) && self.step == Step::Running {
            self.step = Step::Results;
            self.cursor = 0;
        }
    }
}

fn none_if_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}