//! Full-screen TUI front-end for interactive sessions — a Claude-Code-style
//! layout with a scrolling history pane and a pinned bottom input box.
//!
//! The agent's streaming `reply()` events are consumed inline (so the borrow of
//! `agent` and the mutation of `messages` stay disjoint, exactly like the
//! classic readline loop) and rendered into a ratatui scrollback buffer.

mod app;

use std::io::Stdout;
use std::time::Duration;

use anyhow::Result;
use biorouter::agents::{AgentEvent, SessionConfig};
use biorouter::conversation::message::{Message, MessageContent};
use biorouter::permission::permission_confirmation::PrincipalType;
use biorouter::permission::{Permission, PermissionConfirmation};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use rmcp::model::{ErrorCode, ErrorData};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use unicode_width::UnicodeWidthStr;

use self::app::{App, PermissionModal, StatusInfo, ACCENT};
use super::CliSession;

const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Terminal input events are read by a single dedicated task and forwarded over
/// this channel, so the draw/stream loops `recv()` (cancel-safe, buffered)
/// instead of racing multiple `EventStream::next()` futures across `select!`
/// arms — which could drop a partially-decoded keypress (e.g. a mid-stream
/// Ctrl-C).
type Events = mpsc::UnboundedReceiver<Event>;

/// RAII guard owning the alternate screen + raw mode; restores on drop.
struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Tui {
    fn new() -> Result<Self> {
        // Restore the terminal on panic *before* the default hook prints, so a
        // render panic doesn't leave the user in raw mode / alt screen.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(
                std::io::stdout(),
                SetCursorStyle::DefaultUserShape,
                DisableBracketedPaste,
                LeaveAlternateScreen,
                DisableMouseCapture
            );
            prev(info);
        }));

        enable_raw_mode()?;
        let mut out = std::io::stdout();
        // A blinking vertical bar (I-beam) cursor at the insertion point, plus
        // bracketed paste so multi-line pastes arrive as one chunk instead of a
        // flood of keystrokes (which would submit prematurely on embedded \n).
        execute!(
            out,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableBracketedPaste,
            SetCursorStyle::BlinkingBar
        )?;
        let terminal = Terminal::new(CrosstermBackend::new(out))?;
        Ok(Self { terminal })
    }

    fn draw(&mut self, app: &mut App) -> Result<()> {
        self.terminal.draw(|f| draw(f, app))?;
        Ok(())
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            std::io::stdout(),
            SetCursorStyle::DefaultUserShape,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}

/// Run the interactive session in the full-screen TUI.
pub async fn run(session: &mut CliSession, initial_prompt: Option<String>) -> Result<()> {
    let mut tui = Tui::new()?;
    let mut app = App::new(status_from_session(session));
    app.set_catalog(build_catalog(session));
    greeting_into(&mut app);
    push_startup_notices(&mut app).await;
    refresh_context(session, &mut app).await;

    // Single dedicated reader task → channel (see `Events`).
    let (tx, mut rx) = mpsc::unbounded_channel::<Event>();
    let reader = tokio::spawn(async move {
        let mut es = EventStream::new();
        while let Some(Ok(ev)) = es.next().await {
            if tx.send(ev).is_err() {
                break;
            }
        }
    });
    let _reader = AbortOnDropHandle::new(reader);

    if let Some(prompt) = initial_prompt {
        submit(session, &mut app, &mut tui, &mut rx, prompt).await?;
    }

    while !app.should_quit {
        tui.draw(&mut app)?;
        let Some(event) = rx.recv().await else {
            break; // event reader closed
        };
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if let Some(submission) = on_key(&mut app, key) {
                    submit(session, &mut app, &mut tui, &mut rx, submission).await?;
                }
            }
            Event::Paste(s) => app.paste(&s),
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => app.scroll_up(3),
                MouseEventKind::ScrollDown => app.scroll_down(3),
                _ => {}
            },
            _ => {}
        }
    }
    Ok(())
}

/// Handle a key press in the idle (composing) state. Returns the text to submit
/// when the user presses Enter on a non-empty buffer.
fn on_key(app: &mut App, key: KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // The completion popup captures navigation keys first.
    if app.completion_active() {
        match key.code {
            KeyCode::Up => {
                app.completion_move(-1);
                return None;
            }
            KeyCode::Down => {
                app.completion_move(1);
                return None;
            }
            KeyCode::Tab => {
                app.completion_accept();
                app.refresh_completion();
                return None;
            }
            KeyCode::Esc => {
                app.dismiss_completion();
                return None;
            }
            _ => {}
        }
    }

    let mut submit = None;
    match key.code {
        KeyCode::Char('c') if ctrl => {
            if app.input.is_empty() {
                app.should_quit = true;
            } else {
                app.clear_input();
            }
        }
        KeyCode::Char('d') if ctrl => app.should_quit = true,
        KeyCode::Char('j') if ctrl => app.insert_newline(),
        KeyCode::Enter
            if key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT) =>
        {
            app.insert_newline()
        }
        KeyCode::Enter => {
            let text = app.input.trim().to_string();
            if !text.is_empty() {
                app.history.push(text.clone());
                app.clear_input();
                submit = Some(text);
            }
        }
        KeyCode::Tab => app.accept_ghost(),
        KeyCode::Backspace => app.backspace(),
        KeyCode::Left => app.move_left(),
        KeyCode::Right => app.move_right(),
        KeyCode::Home => app.move_home(),
        KeyCode::End => app.move_end(),
        KeyCode::Up if !app.input.contains('\n') => app.history_prev(),
        KeyCode::Down if !app.input.contains('\n') => app.history_next(),
        KeyCode::Up => app.move_left(),
        KeyCode::PageUp => app.scroll_up(10),
        KeyCode::PageDown => app.scroll_down(10),
        KeyCode::Char(c) => app.insert_char(c),
        _ => {}
    }
    // Recompute the popup against the (possibly edited) input.
    app.refresh_completion();
    submit
}

/// Submit a line: handle TUI-local slash commands, else send to the agent.
async fn submit(
    session: &mut CliSession,
    app: &mut App,
    tui: &mut Tui,
    rx: &mut Events,
    text: String,
) -> Result<()> {
    // /compact runs a normal turn whose trigger tells the agent to condense the
    // history (mirrors the classic `/compact`).
    if text.trim() == "/compact" {
        app.push_note("Compacting the conversation…");
        let msg = Message::user().with_text(biorouter::agents::COMPACT_TRIGGERS[0]);
        session.messages.push(msg.clone());
        return drive_response(session, app, tui, rx, msg).await;
    }
    if text.starts_with('/') && handle_slash(session, app, &text).await {
        return Ok(());
    }

    app.push_user(&text);
    let user_message = Message::user().with_text(&text);
    session.messages.push(user_message.clone());
    drive_response(session, app, tui, rx, user_message).await
}

/// Consume the agent's streaming reply, rendering each event into the
/// scrollback while keeping the UI responsive (spinner ticks, scroll, cancel).
async fn drive_response(
    session: &mut CliSession,
    app: &mut App,
    tui: &mut Tui,
    rx: &mut Events,
    user_message: Message,
) -> Result<()> {
    let config = SessionConfig {
        id: session.session_id.clone(),
        schedule_id: session.scheduled_job_id.clone(),
        max_turns: session.max_turns,
        retry_config: session.retry_config.clone(),
    };
    let debug = session.debug;
    let cancel = CancellationToken::new();
    app.thinking = Some(super::thinking::get_random_thinking_message().to_string());

    let mut stream = session
        .agent
        .reply(user_message, config, Some(cancel.clone()))
        .await?;
    let mut tick = tokio::time::interval(Duration::from_millis(110));

    loop {
        tui.draw(app)?;
        tokio::select! {
            _ = tick.tick() => { app.spin = app.spin.wrapping_add(1); }
            ev = rx.recv() => {
                match ev {
                    Some(Event::Key(k)) if k.kind == KeyEventKind::Press => {
                        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                            cancel.cancel();
                        } else if k.code == KeyCode::PageUp { app.scroll_up(10); }
                        else if k.code == KeyCode::PageDown { app.scroll_down(10); }
                    }
                    Some(Event::Mouse(m)) => match m.kind {
                        MouseEventKind::ScrollUp => app.scroll_up(3),
                        MouseEventKind::ScrollDown => app.scroll_down(3),
                        _ => {}
                    },
                    _ => {}
                }
            }
            res = stream.next() => {
                match res {
                    Some(Ok(AgentEvent::Message(message))) => {
                        if let Some((id, prompt)) = super::find_tool_confirmation(&message) {
                            app.push_message(&message, debug);
                            app.thinking = None;
                            let permission = run_permission_modal(app, tui, rx, prompt).await?;
                            if permission == Permission::Cancel {
                                let mut resp = Message::user();
                                resp.content.push(MessageContent::tool_response(
                                    id,
                                    Err(ErrorData {
                                        code: ErrorCode::INVALID_REQUEST,
                                        message: std::borrow::Cow::from("Tool call cancelled by user"),
                                        data: None,
                                    }),
                                ));
                                session.messages.push(resp);
                                app.push_note("Tool call cancelled.");
                                cancel.cancel();
                                // Drain any events the agent already queued so the
                                // in-memory conversation doesn't desync.
                                while stream.next().await.is_some() {}
                                break;
                            }
                            session.agent.handle_confirmation(id, PermissionConfirmation {
                                principal_type: PrincipalType::Tool,
                                permission,
                            }).await;
                            app.thinking = Some(super::thinking::get_random_thinking_message().to_string());
                        } else if super::find_elicitation_request(&message).is_some() {
                            app.push_note("This step needs an interactive form not yet supported in the TUI — cancelling. Use `BIOROUTER_CLI_CLASSIC=1` for that flow.");
                            cancel.cancel();
                            while stream.next().await.is_some() {}
                            break;
                        } else {
                            session.messages.push(message.clone());
                            app.push_message(&message, debug);
                        }
                    }
                    Some(Ok(AgentEvent::McpNotification(_))) => {}
                    Some(Ok(AgentEvent::HistoryReplaced(c))) => { session.messages = c; }
                    Some(Ok(AgentEvent::ModelChange { .. })) => {}
                    Some(Err(e)) => {
                        app.push_error(&e.to_string());
                        cancel.cancel();
                        break;
                    }
                    None => break,
                }
            }
        }
    }

    drop(stream);
    app.thinking = None;
    refresh_context(session, app).await;
    tui.draw(app)?;
    Ok(())
}

/// Show the permission modal and block (within the response loop) until the
/// user chooses an option.
async fn run_permission_modal(
    app: &mut App,
    tui: &mut Tui,
    rx: &mut Events,
    prompt: Option<String>,
) -> Result<Permission> {
    let options: Vec<(&'static str, &'static str)> = if prompt.is_some() {
        vec![
            ("Allow", "allow once"),
            ("Deny", "deny"),
            ("Cancel", "cancel response"),
        ]
    } else {
        vec![
            ("Allow", "allow once"),
            ("Always allow", "always allow"),
            ("Deny", "deny"),
            ("Cancel", "cancel response"),
        ]
    };
    let len = options.len();
    app.modal = Some(PermissionModal {
        prompt,
        options,
        selected: 0,
    });

    let result = loop {
        tui.draw(app)?;
        let k = match rx.recv().await {
            Some(Event::Key(k)) => k,
            Some(_) => continue,
            None => break Permission::Cancel, // event reader closed
        };
        if k.kind != KeyEventKind::Press {
            continue;
        }
        let modal = app.modal.as_mut().unwrap();
        match k.code {
            KeyCode::Up => modal.selected = (modal.selected + len - 1) % len,
            KeyCode::Down => modal.selected = (modal.selected + 1) % len,
            KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                break Permission::Cancel
            }
            KeyCode::Esc => break Permission::Cancel,
            KeyCode::Enter => break label_to_permission(modal.options[modal.selected].0),
            _ => {}
        }
    };
    app.modal = None;
    Ok(result)
}

fn label_to_permission(label: &str) -> Permission {
    match label {
        "Allow" => Permission::AllowOnce,
        "Always allow" => Permission::AlwaysAllow,
        "Deny" => Permission::DenyOnce,
        _ => Permission::Cancel,
    }
}

/// Handle slash commands that the TUI services locally. Returns true if handled.
/// (`/compact` is handled in `submit` since it drives a turn.)
async fn handle_slash(session: &mut CliSession, app: &mut App, text: &str) -> bool {
    match text.trim() {
        "/exit" | "/quit" => {
            app.should_quit = true;
            true
        }
        "/clear" => {
            // Clear the persisted conversation + token counts too, so the
            // session and the context meter don't desync.
            if let Err(e) = session.clear_conversation().await {
                app.push_error(&e.to_string());
            } else {
                app.scrollback.clear();
                app.status.used_tokens = 0;
                greeting_into(app);
            }
            true
        }
        "/help" | "/?" => {
            help_into(app);
            true
        }
        // Agent-serviced commands: let submit() send them through the normal
        // reply flow, where Agent::execute_command handles them.
        s if ["/goal", "/loop", "/schedule", "/prompts", "/prompt"]
            .iter()
            .any(|cmd| s == *cmd || s.starts_with(&format!("{cmd} "))) =>
        {
            false
        }
        other => {
            app.push_note(&format!(
                "`{}` isn't available in the TUI yet — run `BIOROUTER_CLI_CLASSIC=1 biorouter session` for the full command set.",
                other
            ));
            true
        }
    }
}

// ── rendering ────────────────────────────────────────────────────────────────

fn draw(f: &mut Frame, app: &mut App) {
    // Inset the whole UI so nothing renders edge-to-edge: 4 columns of left/right
    // padding and 1 row top/bottom.
    let area = f.area().inner(Margin::new(4, 1));
    let input_lines = app.input.split('\n').count().clamp(1, 6) as u16;
    let input_h = input_lines + 2;
    let gap_h = 2u16; // blank rows separating the response from the input UI
    let status_h = 2u16; // model/provider on line 1; counts + context on line 2
    let hints_h = 1u16;
    // The conversation block hugs the top and grows downward (Claude-style): the
    // history pane is only as tall as its content until it fills the screen,
    // after which it scrolls. A trailing flexible spacer holds it to the top.
    let max_history = area
        .height
        .saturating_sub(gap_h + status_h + input_h + hints_h);
    let history_h = wrapped_count(&app.scrollback, area.width).min(max_history);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(history_h),
            Constraint::Length(gap_h), // breathing room before the input UI
            Constraint::Length(status_h),
            Constraint::Length(input_h),
            Constraint::Length(hints_h),
            Constraint::Min(0), // spacer → anchors the block to the top
        ])
        .split(area);

    draw_history(f, app, chunks[0]);
    draw_status(f, app, chunks[2]);
    draw_input(f, app, chunks[3]);
    draw_hints(f, chunks[4]);

    // The completion popup floats just above the input box.
    if app.completion.is_some() && app.modal.is_none() {
        draw_completion(f, app, chunks[3]);
    }
    if app.modal.is_some() {
        draw_modal(f, app);
    }
}

/// Floating popup listing the matching slash commands and references, anchored
/// just above the input box.
fn draw_completion(f: &mut Frame, app: &App, input_area: Rect) {
    let Some(c) = &app.completion else { return };
    let max_rows = 8usize;
    let rows = c.matches.len().min(max_rows);
    if rows == 0 {
        return;
    }
    let height = rows as u16 + 2; // + borders
    let width = input_area.width.clamp(30, 78);
    let y = input_area.y.saturating_sub(height);
    let rect = Rect {
        x: input_area.x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(ACCENT))
        .title(Span::styled(
            format!(" commands · {} ", c.matches.len()),
            Style::new().fg(ACCENT),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    // Scroll the window so the highlighted row stays visible.
    let start = c.selected.saturating_sub(rows.saturating_sub(1));
    let label_w = c
        .matches
        .iter()
        .skip(start)
        .take(rows)
        .map(|&i| app.catalog[i].label.chars().count())
        .max()
        .unwrap_or(0)
        .min(26);

    let mut lines: Vec<Line> = Vec::new();
    for (vi, &mi) in c.matches.iter().enumerate().skip(start).take(rows) {
        let item = &app.catalog[mi];
        let selected = vi == c.selected;
        let marker = if selected { "▸ " } else { "  " };
        let label_style = if selected {
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::new().fg(ACCENT)),
            Span::styled(
                format!("{:<7}", item.kind.tag()),
                Style::new().add_modifier(Modifier::DIM),
            ),
            Span::styled(format!("{:<label_w$}  ", item.label), label_style),
            Span::styled(
                item.description.clone(),
                Style::new().add_modifier(Modifier::DIM),
            ),
        ]));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner.inner(Margin::new(1, 0)),
    );
}

fn draw_history(f: &mut Frame, app: &mut App, area: Rect) {
    let para = Paragraph::new(app.scrollback.clone()).wrap(Wrap { trim: false });
    let total = wrapped_count(&app.scrollback, area.width);
    app.last_total_lines = total as usize;
    app.last_viewport_h = area.height;
    let base = total.saturating_sub(area.height);
    let offset = base.saturating_sub(app.scroll);
    f.render_widget(para.scroll((offset, 0)), area);
}

/// Estimate how many terminal rows the lines occupy once wrapped to `width`.
/// (`Paragraph::line_count` is private in ratatui, so we approximate; chat
/// content is mostly short lines where this is exact.)
fn wrapped_count(lines: &[Line], width: u16) -> u16 {
    let w = (width.max(1)) as usize;
    let mut total: usize = 0;
    for line in lines {
        // Use unicode *display* width so wide glyphs (CJK = 2 cells) count
        // correctly and the latest output isn't clipped.
        let cells: usize = line
            .spans
            .iter()
            .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        total += cells.div_ceil(w).max(1);
    }
    total.min(u16::MAX as usize) as u16
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let dim = Style::new().add_modifier(Modifier::DIM);
    let sep = || Span::styled("  ·  ", dim);
    let s = &app.status;

    // Line 1: the large-language model and the provider.
    let line1 = Line::from(vec![
        Span::styled(s.model.clone(), Style::new().fg(ACCENT)),
        sep(),
        Span::styled(s.provider.clone(), dim),
    ]);

    // Line 2: skills · extensions · knowledge bases, with the context meter
    // right-aligned.
    let mut line2 = vec![
        Span::styled(format!("{} skills", s.skills), dim),
        sep(),
        Span::styled(format!("{} extensions", s.extensions), dim),
        sep(),
        Span::styled(format!("{} knowledge bases", s.knowledge_bases), dim),
    ];
    let right = context_spans(app);
    let pad = (area.width as usize).saturating_sub(line_width(&line2) + line_width(&right) + 1);
    line2.push(Span::styled(" ".repeat(pad), dim));
    line2.extend(right);

    f.render_widget(Paragraph::new(vec![line1, Line::from(line2)]), area);
}

/// A compact context-window meter: `▕████░░░░▏ 42%`. Empty when the limit is
/// unknown (before the first response).
fn context_spans(app: &App) -> Vec<Span<'static>> {
    let dim = Style::new().add_modifier(Modifier::DIM);
    let limit = app.status.context_limit;
    if limit == 0 {
        return Vec::new();
    }
    let pct = (((app.status.used_tokens as f64 / limit as f64) * 100.0).round() as usize).min(100);
    let width = 12usize;
    let filled = (pct * width / 100).min(width);
    // On-brand until the window is nearly full, then warn (no green).
    let color = if pct < 85 { ACCENT } else { Color::Red };
    vec![
        Span::styled("context ", dim),
        Span::styled("▕", dim),
        Span::styled("█".repeat(filled), Style::new().fg(color)),
        Span::styled("░".repeat(width - filled), dim),
        Span::styled(format!("▏ {}%", pct), dim),
    ]
}

fn draw_hints(f: &mut Frame, area: Rect) {
    let dim = Style::new().add_modifier(Modifier::DIM);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↵ send · ^J newline · / for commands · ↑↓ history · ^C quit",
            dim,
        ))),
        area,
    );
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let (border_color, title) = if let Some(t) = &app.thinking {
        (
            ACCENT,
            format!(" {} {} ", SPINNER[app.spin % SPINNER.len()], t),
        )
    } else {
        (Color::Indexed(240), String::new())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(border_color))
        .title(Span::styled(title, Style::new().fg(ACCENT)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Input text with the coral prompt and dim ghost autofill on the last line.
    // (Suppressed while the completion popup is open — it shows the full list.)
    let ghost = if app.completion.is_some() {
        None
    } else {
        app.ghost()
    };
    let mut lines: Vec<Line> = Vec::new();
    for (i, raw) in app.input.split('\n').enumerate() {
        let prefix = if i == 0 {
            Span::styled("❯ ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
        } else {
            Span::raw("  ")
        };
        let mut spans = vec![prefix, Span::raw(raw.to_string())];
        spans.push(Span::raw(String::new()));
        lines.push(Line::from(spans));
    }
    if let (Some(g), Some(last)) = (ghost, lines.last_mut()) {
        last.spans.push(Span::styled(
            g.to_string(),
            Style::new().add_modifier(Modifier::DIM),
        ));
    }
    if app.input.is_empty() {
        lines = vec![Line::from(vec![
            Span::styled("❯ ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
            // A clearly-greyed placeholder so it doesn't read as real input.
            Span::styled(
                "Send a message…",
                Style::new()
                    .fg(Color::Indexed(244))
                    .add_modifier(Modifier::DIM),
            ),
        ])];
    }
    f.render_widget(Paragraph::new(lines), inner);

    // Place the hardware cursor. Use the unicode *display* width of the text
    // before the cursor so wide glyphs (e.g. CJK, which occupy two cells) keep
    // the caret exactly at the insertion point instead of drifting.
    if app.modal.is_none() {
        let before = app.input.get(..app.cursor).unwrap_or(&app.input);
        let row = before.matches('\n').count() as u16;
        let cur_line = before.rsplit('\n').next().unwrap_or("");
        let col = UnicodeWidthStr::width(cur_line) as u16;
        let x = inner.x + 2 + col; // 2 = "❯ " prompt width
        let y = inner.y + row;
        f.set_cursor_position((x.min(inner.x + inner.width.saturating_sub(1)), y));
    }
}

fn draw_modal(f: &mut Frame, app: &App) {
    let Some(modal) = &app.modal else { return };
    let area = f.area();
    let w = 64u16.min(area.width.saturating_sub(4));
    let h = (modal.options.len() as u16) + 4;
    let rect = Rect {
        x: (area.width.saturating_sub(w)) / 2,
        y: (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(ACCENT))
        .title(Span::styled(
            " Tool approval ",
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines = vec![Line::from(Span::styled(
        modal
            .prompt
            .clone()
            .unwrap_or_else(|| "Biorouter would like to call the above tool.".to_string()),
        Style::default(),
    ))];
    lines.push(Line::default());
    for (i, (label, desc)) in modal.options.iter().enumerate() {
        let selected = i == modal.selected;
        let marker = if selected { "❯ " } else { "  " };
        let style = if selected {
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::new().fg(ACCENT)),
            Span::styled(format!("{:<14}", label), style),
            Span::styled(
                (*desc).to_string(),
                Style::new().add_modifier(Modifier::DIM),
            ),
        ]));
    }
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        inner.inner(Margin::new(1, 0)),
    );
}

fn line_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

// ── content helpers ──────────────────────────────────────────────────────────

fn greeting_into(app: &mut App) {
    app.push_blank();
    for line in crate::session::output::BIOROUTER_ASCII {
        app.push_raw(*line, Style::new().fg(ACCENT).add_modifier(Modifier::BOLD));
    }
    app.push_blank();
    app.push_raw(
        "Biorouter — integrated biomedical research environment",
        Style::new().add_modifier(Modifier::BOLD),
    );
    if !app.status.workdir.is_empty() {
        app.push_line(Line::from(vec![
            Span::styled(
                "working directory: ",
                Style::new().add_modifier(Modifier::DIM),
            ),
            Span::styled(app.status.workdir.clone(), Style::new().fg(ACCENT)),
        ]));
    }
    app.push_raw(
        "Enter your instructions, or type /help for commands",
        Style::new().add_modifier(Modifier::DIM),
    );
}

/// List installed skill slugs (top-level skills and one-level bundles) under the
/// config skills directory, matching how `biorouter skill list` discovers them.
fn list_skills() -> Vec<String> {
    let root = biorouter::config::paths::Paths::config_dir().join("skills");
    fn walk(root: &std::path::Path, dir: &std::path::Path, depth: usize, out: &mut Vec<String>) {
        if depth > 2 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, depth + 1, out);
            } else if p.file_name().and_then(|f| f.to_str()) == Some("SKILL.md") {
                if let Some(rel) = p.parent().and_then(|d| d.strip_prefix(root).ok()) {
                    out.push(rel.display().to_string());
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&root, &root, 0, &mut out);
    out.sort();
    out
}

fn count_skills() -> usize {
    list_skills().len()
}

/// Build the completion catalog shown in the slash-command popup: the TUI slash
/// commands, plus references to skills, extensions, knowledge bases, and
/// extension prompts. Selecting a reference inserts a templated natural-language
/// instruction (mirroring the desktop mention popover).
fn build_catalog(session: &CliSession) -> Vec<app::CompletionItem> {
    use app::{CompletionItem, CompletionKind};
    let mut items: Vec<CompletionItem> = Vec::new();

    let cmd = |name: &str, desc: &str| CompletionItem {
        label: name.to_string(),
        description: desc.to_string(),
        insert: format!("{} ", name),
        filter: name.trim_start_matches('/').to_lowercase(),
        kind: CompletionKind::Command,
    };
    items.push(cmd("/help", "Show commands and shortcuts"));
    items.push(cmd(
        "/compact",
        "Condense the conversation to reclaim context",
    ));
    items.push(cmd("/clear", "Clear the conversation"));
    items.push(cmd("/exit", "Leave the session"));

    // Skills — "Use the \"x\" skill for this request, "
    for slug in list_skills() {
        let name = slug.rsplit('/').next().unwrap_or(&slug).to_string();
        items.push(CompletionItem {
            label: slug.clone(),
            description: "Ask the agent to use this skill".to_string(),
            insert: format!("Use the \"{}\" skill for this request, ", name),
            filter: format!("skill {}", slug.to_lowercase()),
            kind: CompletionKind::Skill,
        });
    }

    // Extensions enabled for this chat.
    for ext in biorouter::config::get_enabled_extensions() {
        let name = ext.name();
        items.push(CompletionItem {
            label: name.clone(),
            description: "Ask the agent to use this extension".to_string(),
            insert: format!("Use the \"{}\" extension for this request, ", name),
            filter: format!("extension {}", name.to_lowercase()),
            kind: CompletionKind::Extension,
        });
    }

    // Knowledge bases visible to the agent.
    if let Ok(svc) = biorouter::knowledge::service::KnowledgeService::new_default() {
        if let Ok(bases) = svc.list_bases() {
            let hidden = svc.get_hidden_persisted().unwrap_or_default();
            for b in bases.into_iter().filter(|b| !hidden.contains(&b.id)) {
                items.push(CompletionItem {
                    label: b.id.clone(),
                    description: format!("Focus knowledge base · {}", b.name),
                    insert: format!(
                        "Using the Knowledge extension, focus the \"{}\" knowledge base for this request, ",
                        b.id
                    ),
                    filter: format!("kb knowledge {} {}", b.id.to_lowercase(), b.name.to_lowercase()),
                    kind: CompletionKind::Knowledge,
                });
            }
        }
    }

    // Extension prompts discovered in the completion cache.
    if let Ok(cache) = session.completion_cache.read() {
        for (name, info) in &cache.prompt_info {
            let desc = info
                .description
                .clone()
                .unwrap_or_else(|| "Extension prompt".to_string());
            items.push(CompletionItem {
                label: name.clone(),
                description: desc,
                insert: format!("Use the \"{}\" prompt for this request, ", name),
                filter: format!("prompt {}", name.to_lowercase()),
                kind: CompletionKind::Prompt,
            });
        }
    }

    items
}

/// Surface the same checks `biorouter doctor` runs — missing required
/// prerequisites and an available update — so the interactive session reflects
/// them too. The update probe is bounded (2s) so it never stalls startup.
async fn push_startup_notices(app: &mut App) {
    let missing: Vec<_> = biorouter::system::check_all()
        .into_iter()
        .filter(|d| d.required && !d.installed)
        .collect();
    if !missing.is_empty() {
        app.push_blank();
        for d in &missing {
            app.push_line(Line::from(vec![
                Span::styled("⚠ ", Style::new().fg(Color::Yellow)),
                Span::styled(format!("{} not found", d.display_name), Style::new().fg(Color::Yellow)),
                Span::styled(
                    "  — run `biorouter doctor` to set up prerequisites".to_string(),
                    Style::new().add_modifier(Modifier::DIM),
                ),
            ]));
        }
    }

    if let Ok(Some(u)) = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        biorouter::system::check_for_update(),
    )
    .await
    {
        if u.update_available {
            app.push_blank();
            app.push_line(Line::from(vec![
                Span::styled("↑ ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("Biorouter {} is available", u.latest),
                    Style::new().fg(ACCENT),
                ),
                Span::styled(
                    format!("  (you have {}) — run `biorouter update`", u.current),
                    Style::new().add_modifier(Modifier::DIM),
                ),
            ]));
        }
    }
}

fn help_into(app: &mut App) {
    app.push_blank();
    app.push_raw(
        "Commands",
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
    );
    for (cmd, desc) in [
        ("/help, /?", "show this help"),
        ("/compact", "condense the conversation to reclaim context"),
        ("/clear", "clear the conversation"),
        (
            "/goal <condition>",
            "keep working until the condition is met",
        ),
        (
            "/loop <interval> <prompt>",
            "run a prompt on an interval (5m, 2h…)",
        ),
        (
            "/schedule <spec> <prompt>",
            "recurring prompt: 5m, @daily, or cron",
        ),
        ("/exit, /quit", "leave the session"),
    ] {
        app.push_line(Line::from(vec![
            Span::styled(format!("  {:<14}", cmd), Style::new().fg(ACCENT)),
            Span::styled(desc.to_string(), Style::new().add_modifier(Modifier::DIM)),
        ]));
    }
    app.push_raw(
        "Navigation",
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
    );
    for (k, d) in [
        ("↵", "send · ⇧↵ or ^J for a newline"),
        ("↑ / ↓", "history · scroll with PageUp/Down or the mouse"),
        ("^C", "clear the line, or quit when empty"),
    ] {
        app.push_line(Line::from(vec![
            Span::styled(format!("  {:<14}", k), Style::new().fg(ACCENT)),
            Span::styled(d.to_string(), Style::new().add_modifier(Modifier::DIM)),
        ]));
    }
}

fn status_from_session(_session: &CliSession) -> StatusInfo {
    let config = biorouter::config::Config::global();

    // Databases = knowledge bases visible to the agent (total minus hidden).
    let databases = biorouter::knowledge::service::KnowledgeService::new_default()
        .ok()
        .and_then(|svc| {
            let bases = svc.list_bases().ok()?;
            let hidden = svc.get_hidden_persisted().unwrap_or_default();
            Some(bases.iter().filter(|b| !hidden.contains(&b.id)).count())
        })
        .unwrap_or(0);

    StatusInfo {
        provider: config.get_biorouter_provider().unwrap_or_default(),
        model: config.get_biorouter_model().unwrap_or_default(),
        workdir: std::env::current_dir()
            .ok()
            .map(|p| shorten_home(&p))
            .unwrap_or_default(),
        skills: count_skills(),
        extensions: biorouter::config::get_enabled_extensions().len(),
        knowledge_bases: databases,
        used_tokens: 0,
        context_limit: 0,
    }
}

/// Replace the home-dir prefix with `~` for a tidier path display.
fn shorten_home(p: &std::path::Path) -> String {
    if let Ok(home) = etcetera::home_dir() {
        if let Ok(rest) = p.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    p.display().to_string()
}

/// Refresh the context-window meter (tokens used / model limit) from the live
/// session. Best-effort: failures leave the meter hidden.
async fn refresh_context(session: &CliSession, app: &mut App) {
    if let Ok(provider) = session.agent.provider().await {
        app.status.context_limit = provider.get_model_config().context_limit();
    }
    if let Ok(meta) = session.get_session().await {
        app.status.used_tokens = meta.total_tokens.unwrap_or(0) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn buffer_text(app: &mut App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn renders_greeting_and_input_without_panic() {
        let mut app = App::new(StatusInfo {
            provider: "versa_azure".into(),
            model: "gpt-5.2".into(),
            skills: 3,
            extensions: 5,
            knowledge_bases: 2,
            ..Default::default()
        });
        greeting_into(&mut app);
        app.push_user("hello there");
        let text = buffer_text(&mut app, 90, 30);
        assert!(text.contains("Biorouter"));
        assert!(text.contains("hello there"));
        assert!(text.contains("gpt-5.2"));
        assert!(text.contains("extensions"));
        assert!(text.contains("knowledge bases"));
        assert!(text.contains("Send a message"));
    }

    #[test]
    fn renders_markdown_message() {
        let mut app = App::new(StatusInfo::default());
        let msg = biorouter::conversation::message::Message::assistant()
            .with_text("# Title\n**bold** and `code`\n- a bullet");
        app.push_message(&msg, false);
        let text = buffer_text(&mut app, 80, 24);
        assert!(text.contains("Title"));
        assert!(text.contains("bold"));
        assert!(text.contains("bullet"));
    }

    #[test]
    fn renders_permission_modal() {
        let mut app = App::new(StatusInfo::default());
        app.modal = Some(PermissionModal {
            prompt: None,
            options: vec![("Allow", "allow once"), ("Deny", "deny")],
            selected: 0,
        });
        let text = buffer_text(&mut app, 80, 24);
        assert!(text.contains("Tool approval"));
        assert!(text.contains("Allow"));
    }

    #[test]
    fn paste_inserts_multiline_without_submitting() {
        let mut app = App::new(StatusInfo::default());
        app.insert_char('a');
        app.paste("line1\r\nline2");
        // \r stripped, \n kept literal (no submit), inserted at the cursor.
        assert_eq!(app.input, "aline1\nline2");
        assert_eq!(app.cursor, app.input.len());
    }

    #[test]
    fn completion_popup_filters_and_accepts() {
        use app::{CompletionItem, CompletionKind};
        let mut app = App::new(StatusInfo::default());
        app.set_catalog(vec![
            CompletionItem {
                label: "/help".into(),
                description: "help".into(),
                insert: "/help ".into(),
                filter: "help".into(),
                kind: CompletionKind::Command,
            },
            CompletionItem {
                label: "/compact".into(),
                description: "compact".into(),
                insert: "/compact ".into(),
                filter: "compact".into(),
                kind: CompletionKind::Command,
            },
        ]);
        // Typing "/" opens the popup with everything.
        app.insert_char('/');
        app.refresh_completion();
        assert!(app.completion_active());
        assert_eq!(app.completion.as_ref().unwrap().matches.len(), 2);
        // Narrowing to "/he" filters to one and accepting fills the input.
        app.insert_char('h');
        app.insert_char('e');
        app.refresh_completion();
        assert_eq!(app.completion.as_ref().unwrap().matches.len(), 1);
        assert!(app.completion_accept());
        assert_eq!(app.input, "/help ");
        assert!(!app.completion_active());
        // The popup renders without panicking.
        let text = buffer_text(
            &mut {
                let mut a = App::new(StatusInfo::default());
                a.set_catalog(vec![CompletionItem {
                    label: "/help".into(),
                    description: "show help".into(),
                    insert: "/help ".into(),
                    filter: "help".into(),
                    kind: CompletionKind::Command,
                }]);
                a.insert_char('/');
                a.refresh_completion();
                a
            },
            80,
            24,
        );
        assert!(text.contains("commands"));
    }

    #[test]
    fn input_editing_and_ghost() {
        let mut app = App::new(StatusInfo::default());
        for c in "/he".chars() {
            app.insert_char(c);
        }
        assert_eq!(app.ghost(), Some("lp"));
        app.accept_ghost();
        assert_eq!(app.input, "/help ");
        app.backspace();
        assert_eq!(app.input, "/help");
    }
}
