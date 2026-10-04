use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::path::{Path, PathBuf};

const WORDLIST_EXTS: [&str; 4] = ["txt", "lst", "dict", "pw"];

#[derive(Clone, Debug)]
pub enum Entry {
    /// Navigate to the parent directory; carries the target path.
    Parent(PathBuf),
    /// A subdirectory; carries its path and display name.
    Dir(PathBuf, String),
    /// A candidate wordlist file; carries its path, name and size in bytes.
    File(PathBuf, String, u64),
}

pub enum BrowserOutcome {
    /// Key consumed, browser stays open.
    None,
    /// User closed the browser without selecting.
    Close,
    /// User selected a wordlist file.
    Selected(PathBuf),
}

/// A minimal directory browser restricted to wordlist-like files, used by the
/// credentials step of the wizard. `for_users` records which field the
/// selected path is written back to.
pub struct Browser {
    pub cwd: PathBuf,
    pub entries: Vec<Entry>,
    pub idx: usize,
    pub for_users: bool,
}

impl Browser {
    pub fn new(for_users: bool) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut b = Self {
            cwd,
            entries: Vec::new(),
            idx: 0,
            for_users,
        };
        b.reload();
        b
    }

    pub fn reload(&mut self) {
        let mut dirs: Vec<(PathBuf, String)> = Vec::new();
        let mut files: Vec<(PathBuf, String, u64)> = Vec::new();

        if let Ok(rd) = std::fs::read_dir(&self.cwd) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let path = entry.path();
                if path.is_dir() {
                    dirs.push((path, name));
                } else {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    if WORDLIST_EXTS.contains(&ext.as_str()) {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        files.push((path, name, size));
                    }
                }
            }
        }

        dirs.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        files.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

        let mut entries = Vec::new();
        if let Some(parent) = self.cwd.parent() {
            if self.cwd != Path::new("/") {
                entries.push(Entry::Parent(parent.to_path_buf()));
            }
        }
        entries.extend(dirs.into_iter().map(|(p, n)| Entry::Dir(p, n)));
        entries.extend(files.into_iter().map(|(p, n, s)| Entry::File(p, n, s)));

        self.entries = entries;
        self.idx = 0;
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> BrowserOutcome {
        if self.entries.is_empty() {
            return match key.code {
                KeyCode::Esc => BrowserOutcome::Close,
                _ => BrowserOutcome::None,
            };
        }
        let last = self.entries.len() - 1;
        match key.code {
            KeyCode::Up => {
                self.idx = self.idx.saturating_sub(1);
                BrowserOutcome::None
            }
            KeyCode::Down => {
                if self.idx < last {
                    self.idx += 1;
                }
                BrowserOutcome::None
            }
            KeyCode::Esc => BrowserOutcome::Close,
            KeyCode::Enter => match &self.entries[self.idx.min(last)] {
                Entry::Parent(p) | Entry::Dir(p, _) => {
                    self.cwd = p.clone();
                    self.reload();
                    BrowserOutcome::None
                }
                Entry::File(p, _, _) => BrowserOutcome::Selected(p.clone()),
            },
            _ => BrowserOutcome::None,
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}