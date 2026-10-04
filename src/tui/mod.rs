pub mod app;
pub mod browser;
pub mod ui;

use anyhow::{Context, Result};
use app::App;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::time::Duration;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

fn init_terminal() -> Result<Tui> {
    ratatui::crossterm::terminal::enable_raw_mode()
        .context("failed to enable raw mode — run `brute` in an interactive terminal, or use a subcommand (brute ftp --help)")?;
    ratatui::crossterm::execute!(
        io::stdout(),
        ratatui::crossterm::terminal::EnterAlternateScreen
    )?;
    let backend = CrosstermBackend::new(io::stdout());
    Ok(Terminal::new(backend)?)
}

fn restore_terminal() -> Result<()> {
    ratatui::crossterm::terminal::disable_raw_mode()?;
    ratatui::crossterm::execute!(
        io::stdout(),
        ratatui::crossterm::terminal::LeaveAlternateScreen
    )?;
    Ok(())
}

/// Launch the interactive wizard. Runs inside the tokio runtime so the
/// attack workers spawned from the Confirm step share the event loop.
pub async fn run() -> Result<()> {
    // Make sure the terminal is restored even if the UI panics.
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        orig_hook(info);
    }));

    let mut terminal = init_terminal()?;
    let mut app = App::new();

    let res = event_loop(&mut terminal, &mut app).await;

    restore_terminal()?;
    let _ = std::panic::take_hook(); // drop our hook; default is restored
    res
}

async fn event_loop(t: &mut Tui, app: &mut App) -> Result<()> {
    loop {
        t.draw(|f| ui::render(f, app))?;
        app.tick();

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    // Treat Ctrl+D as an extra quit shortcut on any screen.
                    if key.code == KeyCode::Char('d')
                        && key
                            .modifiers
                            .contains(ratatui::crossterm::event::KeyModifiers::CONTROL)
                    {
                        app.request_quit();
                    } else {
                        app.on_key(key);
                    }
                }
            }
        }

        app.poll_attack();

        if app.should_quit {
            return Ok(());
        }
    }
}