use anyhow::{Context, Result};
use std::path::Path;

/// Load a wordlist (one entry per line). Leading/trailing whitespace is trimmed
/// and empty lines are skipped.
pub fn load_words(path: &str) -> Result<Vec<String>> {
    let data = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read wordlist: {path}"))?;
    let words: Vec<String> = data
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if words.is_empty() {
        anyhow::bail!("wordlist is empty: {path}");
    }
    Ok(words)
}

/// Treat an argument as either a literal value or a path to a wordlist.
/// If the argument names an existing file it is loaded; otherwise it is
/// treated as a single literal entry. Empty values are rejected.
pub fn resolve_words(arg: &str) -> Result<Vec<String>> {
    let trimmed = arg.trim();
    if trimmed.is_empty() {
        anyhow::bail!("empty value — provide a wordlist path or a literal entry");
    }
    if Path::new(trimmed).is_file() {
        load_words(trimmed)
    } else {
        Ok(vec![trimmed.to_string()])
    }
}