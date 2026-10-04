use crate::tui::app::{App, Step};
use crate::tui::browser::{human_size, Browser, Entry};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Gauge, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

fn sel() -> Style {
    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}
fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}
fn txt() -> Style {
    Style::new().fg(Color::White)
}
fn ok() -> Style {
    Style::new().fg(Color::Green)
}
fn err() -> Style {
    Style::new().fg(Color::Red)
}
fn cyan() -> Style {
    Style::new().fg(Color::Cyan)
}
fn green_bold() -> Style {
    Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
}

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);

    render_header(f, chunks[0], app);
    match app.step {
        Step::Method => render_method(f, chunks[1], app),
        Step::Target => render_target(f, chunks[1], app),
        Step::Credentials => render_creds(f, chunks[1], app),
        Step::Options => render_options(f, chunks[1], app),
        Step::Confirm => render_confirm(f, chunks[1], app),
        Step::Running => render_running(f, chunks[1], app),
        Step::Results => render_results(f, chunks[1], app),
    }
    render_footer(f, chunks[2], app);

    if let Some(b) = &app.browser {
        render_browser(f, area, b);
    }
}

// -------------------------------------------------------------------- header

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let left = match &app.run {
        Some(r) if app.step == Step::Running || app.step == Step::Results => {
            format!(" {} ", r.target_label)
        }
        _ => " brute — authorized credential testing ".to_string(),
    };

    let mut pipeline: Vec<Span> = vec![];
    for (i, name) in ["Method", "Target", "Credentials", "Options", "Run"]
        .iter()
        .enumerate()
    {
        if i > 0 {
            pipeline.push(Span::styled(" › ", dim()));
        }
        let current = app.step.index() == i + 1 && app.step.index() > 0;
        pipeline.push(Span::styled(*name, if current { sel() } else { dim() }));
    }

    let block = Block::bordered()
        .border_style(cyan())
        .title_top(Line::from(left).left_aligned())
        .title_top(
            Line::from(" use only on systems you are authorized to test ".to_string())
                .right_aligned(),
        );

    let inner = block.inner(area);
    f.render_widget(block, area);
    let text = Paragraph::new(Line::from(pipeline).centered());
    f.render_widget(text, inner);
}

// -------------------------------------------------------------------- footer

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered().border_style(Style::new().fg(Color::DarkGray));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let hints: &str = match app.step {
        Step::Method => "↑/↓ select · Enter confirm · q quit",
        Step::Target => "↑/↓ fields · type to edit · Enter next · Esc back · Ctrl+C quit",
        Step::Credentials => "↑/↓ fields · Enter browse wordlists · Esc back",
        Step::Options => "↑/↓ fields · Enter toggle/next · ←/→ change auth · Esc back",
        Step::Confirm => "←/→ choose · Enter confirm · Esc back",
        Step::Running => "Esc cancel run · q quit",
        Step::Results => "←/→ choose · Enter confirm · q quit",
    };

    let msg_line = if app.browser.is_some() {
        Line::from(Span::styled(
            "↑/↓ navigate · Enter select/enter dir · Esc close",
            ok(),
        ))
    } else {
        match &app.msg {
            Some((m, _, true)) => Line::from(Span::styled(format!("⚠ {m}"), err())),
            Some((m, _, false)) => Line::from(Span::styled(format!("ℹ {m}"), ok())),
            None => Line::from(""),
        }
    };

    let text = Paragraph::new(vec![msg_line, Line::from(Span::styled(hints, dim()))]);
    f.render_widget(text, inner);
}

// -------------------------------------------------------------------- fields

fn marker(selected: bool) -> Span<'static> {
    if selected {
        Span::styled("❯ ", sel())
    } else {
        Span::styled("  ", dim())
    }
}

#[allow(clippy::too_many_arguments)]
fn text_field<'a>(
    field_name: &str,
    value: &'a str,
    placeholder: &'a str,
    selected: bool,
    note: Option<(&'a str, bool)>,
) -> Line<'a> {
    let mut spans = vec![marker(selected)];
    spans.push(Span::styled(
        format!("{field_name:<18}"),
        if selected { sel() } else { txt() },
    ));
    if value.is_empty() {
        spans.push(Span::styled(placeholder, dim()));
    } else {
        spans.push(Span::raw(value));
    }
    if selected {
        spans.push(Span::styled("▌", sel()));
    }
    if let Some((note, is_err)) = note {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            if is_err {
                format!("✗ {note}")
            } else {
                format!("✓ {note}")
            },
            if is_err { err() } else { ok() },
        ));
    }
    Line::from(spans)
}

fn toggle_field(field_name: &str, on: bool, selected: bool, hint: &str) -> Line<'static> {
    let mut spans = vec![marker(selected)];
    spans.push(Span::styled(
        format!("{field_name:<18}"),
        if selected { sel() } else { txt() },
    ));
    spans.push(Span::styled(
        if on { "[x] yes" } else { "[ ] no" },
        if on { ok() } else { dim() },
    ));
    if !hint.is_empty() {
        spans.push(Span::styled(format!("   {hint}"), dim()));
    }
    Line::from(spans)
}

fn cycle_field(field_name: &str, current: &str, selected: bool) -> Line<'static> {
    let mut spans = vec![marker(selected)];
    spans.push(Span::styled(
        format!("{field_name:<18}"),
        if selected { sel() } else { txt() },
    ));
    spans.push(Span::styled(format!("‹ {current} ›"), cyan()));
    spans.push(Span::styled("   Enter or ←/→ to change", dim()));
    Line::from(spans)
}

fn button(btn_label: &str, selected: bool) -> Span<'static> {
    if selected {
        Span::styled(format!("❯ [ {btn_label} ]"), sel())
    } else {
        Span::styled(format!("  [ {btn_label} ]  "), dim())
    }
}

// -------------------------------------------------------------------- steps

fn render_method(f: &mut Frame, area: Rect, app: &App) {
    let methods = [
        crate::attack::Method::Ftp,
        crate::attack::Method::Ssh,
        crate::attack::Method::Http,
    ];
    let items: Vec<ListItem> = methods
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let selected = i == app.method_idx;
            let line = Line::from(vec![
                marker(selected),
                Span::styled(
                    format!("{:<6}", m.label()),
                    if selected { sel() } else { txt() },
                ),
                Span::styled(m.description(), if selected { txt() } else { dim() }),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(
        Block::bordered()
            .border_style(cyan())
            .title_top(Line::from(" Select target protocol ".to_string()).centered()),
    );
    f.render_widget(list, area);
}

fn render_target(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .border_style(cyan())
        .title_top(Line::from(" Target ".to_string()).centered());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let fc = app.field_count();
    let mut lines: Vec<Line> = vec![];
    if app.is_http() {
        lines.push(text_field(
            "URL",
            &app.host,
            "http://10.0.0.5/login",
            app.cursor == 0,
            None,
        ));
        lines.push(Line::from(Span::styled(
            "    full URL incl. scheme and path — port goes in the URL (e.g. :8080)",
            dim(),
        )));
    } else {
        lines.push(text_field(
            "Host / IP",
            &app.host,
            "10.0.0.5 or example.com",
            app.cursor == 0,
            None,
        ));
        lines.push(text_field("Port", &app.port, "", app.cursor == 1, None));
        lines.push(Line::from(Span::styled(
            format!(
                "    default port for {} is {}",
                app.method.label(),
                app.method.default_port()
            ),
            dim(),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(button("Continue", app.cursor + 1 == fc)));

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_creds(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .border_style(cyan())
        .title_top(Line::from(" Credentials ".to_string()).centered());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let users_count = app
        .users
        .as_ref()
        .map(|v| format!("{} entries", v.len()));
    let pass_count = app
        .passwords
        .as_ref()
        .map(|v| format!("{} entries", v.len()));

    let users_note: Option<(&str, bool)> = match (&users_count, &app.users_err) {
        (Some(c), _) => Some((c.as_str(), false)),
        (None, Some(e)) => Some((e.as_str(), true)),
        _ => None,
    };
    let pass_note: Option<(&str, bool)> = match (&pass_count, &app.pass_err) {
        (Some(c), _) => Some((c.as_str(), false)),
        (None, Some(e)) => Some((e.as_str(), true)),
        _ => None,
    };

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        "Enter opens a wordlist browser — or type a file path / single literal value.",
        dim(),
    ))];
    lines.push(Line::from(""));
    lines.push(text_field(
        "Users",
        &app.users_path,
        "path to username list, or a literal user",
        app.cursor == 0,
        users_note,
    ));
    lines.push(text_field(
        "Passwords",
        &app.pass_path,
        "path to password list, or a literal password",
        app.cursor == 1,
        pass_note,
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(button("Continue", app.cursor == 2)));

    f.render_widget(Paragraph::new(lines), inner);
}

fn render_options(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .border_style(cyan())
        .title_top(Line::from(" Options ".to_string()).centered());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let http = app.is_http();
    let fc = app.field_count();
    let mut lines: Vec<Line> = vec![];

    lines.push(text_field("Threads", &app.threads, "", app.cursor == 0, None));
    lines.push(text_field("Timeout (s)", &app.timeout, "", app.cursor == 1, None));
    lines.push(text_field("Delay (ms)", &app.delay, "", app.cursor == 2, None));
    lines.push(toggle_field(
        "Stop on success",
        app.stop_on_success,
        app.cursor == 3,
        "stop the run after the first valid credential",
    ));

    if http {
        lines.push(Line::from(Span::styled(
            "────────── HTTP options ──────────",
            dim(),
        )));
        lines.push(cycle_field(
            "Auth type",
            if app.http_auth == crate::cli::HttpAuth::Basic {
                "basic"
            } else {
                "form (POST)"
            },
            app.cursor == 4,
        ));
        lines.push(text_field("User field", &app.user_field, "", app.cursor == 5, None));
        lines.push(text_field("Pass field", &app.pass_field, "", app.cursor == 6, None));
        lines.push(text_field(
            "Success text",
            &app.success_text,
            "marker present in response when login OK",
            app.cursor == 7,
            None,
        ));
        lines.push(text_field("Success code", &app.success_code, "", app.cursor == 8, None));
        lines.push(text_field(
            "Fail pattern",
            &app.fail_pattern,
            "marker present in response when login fails",
            app.cursor == 9,
            None,
        ));
        lines.push(toggle_field(
            "Follow redirects",
            app.follow,
            app.cursor == 10,
            "",
        ));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(button("Continue", app.cursor + 1 == fc)));

    f.render_widget(Paragraph::new(lines), inner);
}

fn pair(name: &str, value: impl AsRef<str>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{name:<16}"), dim()),
        Span::styled(value.as_ref().to_string(), txt()),
    ])
}

fn short(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn render_confirm(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .border_style(cyan())
        .title_top(Line::from(" Confirm & start ".to_string()).centered());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let users = app.users.as_ref().map(|v| v.len()).unwrap_or(0);
    let passwords = app.passwords.as_ref().map(|v| v.len()).unwrap_or(0);
    let target = if app.is_http() {
        app.host.clone()
    } else {
        format!("{}:{}", app.host, app.port)
    };

    let mut lines: Vec<Line> = vec![
        pair("Method", app.method.label()),
        pair("Target", target),
        pair(
            "Users",
            format!("{} entries ({})", users, short(&app.users_path)),
        ),
        pair(
            "Passwords",
            format!("{} entries ({})", passwords, short(&app.pass_path)),
        ),
        pair("Combinations", users.saturating_mul(passwords).to_string()),
        pair("Threads", app.threads.trim().to_string()),
        pair("Timeout", format!("{}s", app.timeout.trim())),
        pair("Delay", format!("{}ms", app.delay.trim())),
        pair(
            "Stop on success",
            if app.stop_on_success { "yes" } else { "no" },
        ),
    ];

    if app.is_http() {
        lines.push(pair(
            "HTTP auth",
            if app.http_auth == crate::cli::HttpAuth::Basic {
                "basic"
            } else {
                "form (POST)"
            },
        ));
        lines.push(pair(
            "Form fields",
            format!("{} / {}", app.user_field, app.pass_field),
        ));
        if !app.success_text.trim().is_empty() {
            lines.push(pair("Success text", app.success_text.trim().to_string()));
        }
        lines.push(pair("Success code", app.success_code.trim().to_string()));
        lines.push(pair(
            "Follow redirects",
            if app.follow { "yes" } else { "no" },
        ));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        button("Start attack", app.cursor == 0),
        Span::raw("    "),
        button("Back", app.cursor == 1),
    ]));

    f.render_widget(Paragraph::new(lines), inner);
}

// -------------------------------------------------------------------- running

fn render_running(f: &mut Frame, area: Rect, app: &App) {
    let Some(run) = &app.run else { return };

    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Min(1),
    ])
    .split(area);

    // Target + status line
    let status = if run.cancelling {
        Span::styled("   cancelling…", err())
    } else {
        Span::styled("   running", ok())
    };
    let target_line = Line::from(vec![
        Span::styled(" target  ", dim()),
        Span::styled(run.target_label.clone(), txt()),
        status,
    ]);
    f.render_widget(Paragraph::new(target_line), rows[0]);

    // Progress gauge
    let total = run.total.max(1);
    let ratio = (run.done as f64 / total as f64).clamp(0.0, 1.0);
    let pct = (ratio * 1000.0).round() / 10.0;
    let gauge = Gauge::default()
        .block(
            Block::bordered()
                .border_style(Style::new().fg(Color::DarkGray))
                .title_top(Line::from(" progress ".to_string()).centered()),
        )
        .gauge_style(Style::new().fg(Color::Green))
        .ratio(ratio)
        .label(Span::from(format!(
            " {}/{} ({pct}%) ",
            run.done, run.total
        )));
    f.render_widget(gauge, rows[1]);

    // Stats line
    let elapsed = run.started.elapsed().as_secs();
    let rate = if elapsed > 0 {
        run.done as f64 / elapsed as f64
    } else {
        run.done as f64
    };
    let stats = Line::from(vec![
        Span::styled(" tried ", dim()),
        Span::styled(format!("{}", run.done), txt()),
        Span::styled(" · found ", dim()),
        Span::styled(
            format!("{}", run.found.len()),
            if run.found.is_empty() {
                dim()
            } else {
                green_bold()
            },
        ),
        Span::styled(" · errors ", dim()),
        Span::styled(format!("{}", run.errors), txt()),
        Span::styled(" · ", dim()),
        Span::styled(format!("{rate:.0}/s"), txt()),
        Span::styled(" · ", dim()),
        Span::styled(format!("{}:{:02}", elapsed / 60, elapsed % 60), txt()),
    ]);
    f.render_widget(Paragraph::new(stats), rows[2]);

    // Found credentials list
    let items: Vec<ListItem> = if run.found.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "no valid credentials yet",
            dim(),
        )))]
    } else {
        run.found
            .iter()
            .map(|c| {
                ListItem::new(Line::from(vec![
                    Span::styled(" ✓ ", green_bold()),
                    Span::styled(format!("{} : {}", c.user, c.pass), txt()),
                    Span::styled(format!("   {}", c.detail), dim()),
                ]))
            })
            .collect()
    };
    let list = List::new(items).block(
        Block::bordered()
            .border_style(Style::new().fg(Color::Green))
            .title_top(Line::from(" found credentials ".to_string()).centered()),
    );

    // Follow the most recent find so newly discovered credentials scroll
    // into view instead of being clipped at the bottom.
    let mut state = ListState::default();
    if !run.found.is_empty() {
        state.select(Some(run.found.len() - 1));
    }
    f.render_stateful_widget(list, rows[3], &mut state);
}

// -------------------------------------------------------------------- results

fn render_results(f: &mut Frame, area: Rect, app: &App) {
    let Some(run) = &app.run else { return };
    let Some(fin) = &run.finished else { return };

    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);

    let outcome = if fin.cancelled {
        Span::styled("cancelled", err())
    } else if fin.found > 0 {
        Span::styled("finished", green_bold())
    } else {
        Span::styled("finished — no valid credentials", err())
    };
    let elapsed = run.started.elapsed().as_secs_f64();
    let summary = vec![
        Line::from(vec![
            Span::styled(" run ", dim()),
            Span::styled(run.target_label.clone(), txt()),
            Span::styled("  ", dim()),
            outcome,
        ]),
        Line::from(vec![
            Span::styled(
                format!(
                    " {} valid credential(s) · {} attempts ",
                    fin.found, fin.attempted
                ),
                txt(),
            ),
            Span::styled(format!("· {:.1}s", elapsed), dim()),
        ]),
    ];
    f.render_widget(Paragraph::new(summary), rows[0]);

    let items: Vec<ListItem> = if run.found.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "— nothing found —",
            dim(),
        )))]
    } else {
        run.found
            .iter()
            .map(|c| {
                ListItem::new(Line::from(vec![
                    Span::styled(" ✓ ", green_bold()),
                    Span::styled(format!("{} : {}", c.user, c.pass), txt()),
                    Span::styled(format!("   {}", c.detail), dim()),
                ]))
            })
            .collect()
    };
    let list = List::new(items).block(
        Block::bordered()
            .border_style(cyan())
            .title_top(Line::from(" results ".to_string()).centered()),
    );

    // Scroll to the end of the results list when it overflows.
    let mut state = ListState::default();
    if !run.found.is_empty() {
        state.select(Some(run.found.len() - 1));
    }
    f.render_stateful_widget(list, rows[1], &mut state);

    let buttons = Line::from(vec![
        button("New run", app.cursor == 0),
        Span::raw("    "),
        button("Quit", app.cursor == 1),
    ]);
    f.render_widget(Paragraph::new(buttons), rows[2]);
}

// -------------------------------------------------------------------- browser

fn render_browser(f: &mut Frame, area: Rect, b: &Browser) {
    let w = 64u16.min(area.width.saturating_sub(2));
    let h = 18u16.min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let rect = Rect::new(x, y, w, h);

    f.render_widget(Clear, rect);

    let items: Vec<ListItem> = if b.entries.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "no wordlist files here (.txt .lst .dict .pw) — try the parent directory",
            dim(),
        )))]
    } else {
        b.entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let selected = i == b.idx;
                let text = match e {
                    Entry::Parent(_) => "‥  parent directory".to_string(),
                    Entry::Dir(_, n) => format!("📁 {n}/"),
                    Entry::File(_, n, s) => format!("📄 {n}  ({})", human_size(*s)),
                };
                let style = match e {
                    Entry::File(..) => txt(),
                    _ => dim(),
                };
                ListItem::new(Line::from(vec![
                    marker(selected),
                    Span::styled(text, if selected { sel() } else { style }),
                ]))
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::bordered()
            .border_style(Style::new().fg(Color::Yellow))
            .title(format!(" wordlist — {} ", b.cwd.display())),
    );

    // Stateful rendering keeps the selected entry scrolled into view when
    // the directory has more entries than fit in the popup.
    let mut state = ListState::default();
    if !b.entries.is_empty() {
        state.select(Some(b.idx.min(b.entries.len() - 1)));
    }
    f.render_stateful_widget(list, rect, &mut state);
}