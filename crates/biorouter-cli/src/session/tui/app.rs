//! TUI application state: the scrollback buffer, the input editor, scroll
//! position, status line, and the helpers that turn agent messages into styled
//! ratatui lines (with a lightweight markdown renderer).

use std::collections::VecDeque;

use biorouter::conversation::message::{Message, MessageContent};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::session::completion::SLASH_COMMANDS;

/// Brand warm tan-brown accent (xterm-256 137 ≈ #af875f), Biorouter's light cream palette
pub const ACCENT: Color = Color::Indexed(137);
const DIM: Style = Style::new().add_modifier(Modifier::DIM);
/// Subtle slate fill behind the user's own messages so a turn the *user* sent is
/// instantly distinguishable from the agent's reply (Claude-Code-style block).
const USER_BG: Color = Color::Indexed(237);
const USER_FG: Color = Color::Indexed(252);

/// A modal asking the user to approve a tool call.
pub struct PermissionModal {
    pub prompt: Option<String>,
    pub options: Vec<(&'static str, &'static str)>,
    pub selected: usize,
}

/// What category a completion entry is (drives its colored tag in the popup).
#[derive(Clone, Copy, PartialEq)]
pub enum CompletionKind {
    Command,
    Skill,
    Extension,
    Knowledge,
    Prompt,
}

impl CompletionKind {
    pub fn tag(self) -> &'static str {
        match self {
            CompletionKind::Command => "cmd",
            CompletionKind::Skill => "skill",
            CompletionKind::Extension => "ext",
            CompletionKind::Knowledge => "kb",
            CompletionKind::Prompt => "prompt",
        }
    }
}

/// One entry in the slash-command / reference popup.
#[derive(Clone)]
pub struct CompletionItem {
    /// Shown in the list, e.g. `/help` or `clinvar-lookup`.
    pub label: String,
    pub description: String,
    /// What replaces the input buffer when the entry is accepted.
    pub insert: String,
    /// Lowercase text the typed query is matched against.
    pub filter: String,
    pub kind: CompletionKind,
}

/// Live popup state: which catalog entries currently match, and the highlight.
pub struct Completion {
    pub matches: Vec<usize>,
    pub selected: usize,
}

/// One-line status info shown above the input box.
#[derive(Clone, Default)]
pub struct StatusInfo {
    pub provider: String,
    pub model: String,
    pub workdir: String,
    pub skills: usize,
    pub extensions: usize,
    pub knowledge_bases: usize,
    /// Tokens used so far this session, and the model's context window.
    pub used_tokens: usize,
    pub context_limit: usize,
}

pub struct App {
    /// Rendered history, one styled line per entry (newest at the end).
    pub scrollback: Vec<Line<'static>>,
    /// Current input buffer (may contain `\n` for multi-line input).
    pub input: String,
    /// Cursor position as a byte offset into `input`.
    pub cursor: usize,
    /// Past submissions for Up/Down recall.
    pub history: Vec<String>,
    pub hist_idx: Option<usize>,
    /// Lines scrolled up from the bottom (0 = pinned to the latest output).
    pub scroll: u16,
    pub status: StatusInfo,
    /// Active "thinking" spinner message, or `None` when idle.
    pub thinking: Option<String>,
    pub spin: usize,
    pub modal: Option<PermissionModal>,
    pub should_quit: bool,
    /// Last rendered history-viewport height, for page scrolling.
    pub last_viewport_h: u16,
    pub last_total_lines: usize,
    /// Cache of draw_history's wrapped-line total, keyed by a cheap content
    /// signature (scrollback length, viewport width, in-progress stream length).
    /// Skips the O(content) unicode re-measure on redraws where nothing changed
    /// (e.g. spinner-tick redraws during streaming). (sb_len, width, stream_len, total)
    pub(super) wrap_cache: Option<(usize, u16, usize, u16)>,
    /// Full set of completable commands / references (built once at startup).
    pub catalog: Vec<CompletionItem>,
    /// Active completion popup, recomputed as the input changes.
    pub completion: Option<Completion>,
    /// Set when the user presses Esc; suppresses the popup until the next edit.
    pub completion_dismissed: bool,
    /// Submissions typed while a response was streaming, sent in order once the
    /// current turn finishes (lets the user keep typing instead of being locked
    /// out while the agent works).
    pub queued: VecDeque<String>,
    /// Live token-streaming state: the in-progress assistant text, its response
    /// id, and the scrollback index where its preview begins — so each delta
    /// re-renders in place and the finished text commits as proper Markdown.
    pub stream_text: String,
    pub stream_id: Option<String>,
    pub stream_start: Option<usize>,
    /// Scrollback range `[start, start+len)` of the MOST RECENT tool result's
    /// preview lines. When the next tool call starts, that block is collapsed to a
    /// one-line summary so old tool output doesn't bury the conversation — only the
    /// current tool result stays expanded. `None` when there's no collapsible block
    /// (or in debug mode, where full output is kept).
    pub last_tool_result: Option<(usize, usize)>,
}

impl App {
    pub fn new(status: StatusInfo) -> Self {
        Self {
            scrollback: Vec::new(),
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            hist_idx: None,
            scroll: 0,
            status,
            thinking: None,
            spin: 0,
            modal: None,
            should_quit: false,
            last_viewport_h: 0,
            last_total_lines: 0,
            wrap_cache: None,
            catalog: Vec::new(),
            completion: None,
            completion_dismissed: false,
            queued: VecDeque::new(),
            stream_text: String::new(),
            stream_id: None,
            stream_start: None,
            last_tool_result: None,
        }
    }

    // ── live token streaming ─────────────────────────────────────────────────

    /// Append a streamed assistant-text delta and re-render the live preview in
    /// place (as Markdown, so a finished structure snaps into shape as it
    /// completes). A new response id while one is in flight commits the prior.
    pub fn stream_delta(&mut self, id: Option<String>, delta: &str) {
        if self.stream_start.is_some() && id.is_some() && self.stream_id != id {
            self.stream_commit();
        }
        if self.stream_start.is_none() {
            self.push_blank();
            self.stream_start = Some(self.scrollback.len());
            self.stream_id = id;
            self.stream_text.clear();
        }
        self.stream_text.push_str(delta);
        if let Some(start) = self.stream_start {
            self.scrollback.truncate(start);
            for line in md_lines(&self.stream_text) {
                self.scrollback.push(line);
            }
        }
        self.scroll = 0; // follow the latest output
    }

    /// Finalize the streamed message into permanent scrollback. Returns the full
    /// text when non-empty assistant text was streamed (for the session mirror).
    pub fn stream_commit(&mut self) -> Option<String> {
        let start = self.stream_start.take()?;
        self.stream_id = None;
        self.scrollback.truncate(start);
        let text = std::mem::take(&mut self.stream_text);
        if text.trim().is_empty() {
            return None;
        }
        for line in md_lines(&text) {
            self.push_line(line);
        }
        Some(text)
    }

    pub fn set_catalog(&mut self, items: Vec<CompletionItem>) {
        self.catalog = items;
    }

    // ── completion popup ────────────────────────────────────────────────────

    pub fn completion_active(&self) -> bool {
        self.completion.is_some()
    }

    /// Recompute the popup from the current input. The popup opens whenever the
    /// buffer is a single `/`-prefixed token (no space yet); typing just `/`
    /// lists everything.
    pub fn refresh_completion(&mut self) {
        self.completion = None;
        if self.completion_dismissed {
            return;
        }
        let line = &self.input;
        if !line.starts_with('/') || line.contains(' ') || line.contains('\n') {
            return;
        }
        let q = line.strip_prefix('/').unwrap_or(line).to_lowercase();
        let mut matches: Vec<usize> = self
            .catalog
            .iter()
            .enumerate()
            .filter(|(_, it)| q.is_empty() || it.filter.contains(&q))
            .map(|(i, _)| i)
            .collect();
        // Prefix matches first, then by kind order, then alphabetically.
        matches.sort_by(|&a, &b| {
            let ia = &self.catalog[a];
            let ib = &self.catalog[b];
            let pa = u8::from(!ia.filter.starts_with(&q));
            let pb = u8::from(!ib.filter.starts_with(&q));
            pa.cmp(&pb)
                .then((ia.kind as u8).cmp(&(ib.kind as u8)))
                .then(ia.label.cmp(&ib.label))
        });
        if !matches.is_empty() {
            self.completion = Some(Completion {
                matches,
                selected: 0,
            });
        }
    }

    pub fn completion_move(&mut self, delta: i32) {
        if let Some(c) = &mut self.completion {
            let n = c.matches.len() as i32;
            if n > 0 {
                c.selected = (((c.selected as i32 + delta) % n + n) % n) as usize;
            }
        }
    }

    /// Apply the highlighted entry to the input buffer. Returns true if applied.
    pub fn completion_accept(&mut self) -> bool {
        let insert = self.completion.as_ref().and_then(|c| {
            c.matches
                .get(c.selected)
                .and_then(|&i| self.catalog.get(i))
                .map(|it| it.insert.clone())
        });
        if let Some(insert) = insert {
            self.input = insert;
            self.cursor = self.input.len();
            self.completion = None;
            return true;
        }
        false
    }

    pub fn dismiss_completion(&mut self) {
        self.completion = None;
        self.completion_dismissed = true;
    }

    // ── scrollback ──────────────────────────────────────────────────────────

    pub fn push_line(&mut self, line: Line<'static>) {
        self.scrollback.push(line);
        self.scroll = 0; // follow the latest output
    }

    pub fn push_blank(&mut self) {
        if !matches!(self.scrollback.last(), Some(l) if l.spans.is_empty()) {
            self.push_line(Line::default());
        }
    }

    pub fn push_raw<S: Into<String>>(&mut self, s: S, style: Style) {
        self.push_line(Line::from(Span::styled(s.into(), style)));
    }

    /// Append the user's submitted text as a left-barred, softly-shaded block so
    /// it reads as clearly the user's own turn (vs. the agent's plain reply).
    pub fn push_user(&mut self, text: &str) {
        self.push_blank();
        let bar = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);
        let body = Style::new().fg(USER_FG).bg(USER_BG);
        for raw in text.lines() {
            self.push_line(Line::from(vec![
                Span::styled("▌ ", bar),
                // Trailing space extends the shading a touch past the text.
                Span::styled(format!("{} ", raw), body),
            ]));
        }
        self.push_blank();
    }

    pub fn push_note(&mut self, text: &str) {
        self.push_line(Line::from(Span::styled(
            text.to_string(),
            Style::new().fg(Color::Yellow),
        )));
    }

    pub fn push_error(&mut self, text: &str) {
        self.push_blank();
        self.push_line(Line::from(vec![
            Span::styled(
                "error: ",
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(text.to_string(), Style::new().fg(Color::Red)),
        ]));
    }

    /// Render an agent message into the scrollback.
    pub fn push_message(&mut self, message: &Message, debug: bool) {
        for content in &message.content {
            match content {
                MessageContent::Text(t) => {
                    if !t.text.trim().is_empty() {
                        self.push_blank();
                        for line in md_lines(&t.text) {
                            self.push_line(line);
                        }
                    }
                }
                MessageContent::ToolRequest(req) => {
                    if let Ok(call) = &req.tool_call {
                        self.push_tool_header(&call.name);
                        if let Some(args) = &call.arguments {
                            for (k, v) in args.iter() {
                                let val = compact_value(v);
                                self.push_line(Line::from(vec![
                                    Span::styled(format!("    {}: ", k), DIM),
                                    Span::raw(val),
                                ]));
                            }
                        }
                    }
                }
                MessageContent::ToolResponse(resp) => {
                    if let Ok(result) = &resp.tool_result {
                        let start = self.scrollback.len();
                        for c in &result.content {
                            if let Some(text) = c.as_text() {
                                let preview = if debug {
                                    text.text.clone()
                                } else {
                                    truncate_lines(&text.text, 12)
                                };
                                for l in preview.lines() {
                                    self.push_line(Line::from(Span::styled(l.to_string(), DIM)));
                                }
                            }
                        }
                        // Remember this block so the NEXT tool call can collapse it.
                        // Skip in debug (full output is intentionally kept there).
                        let count = self.scrollback.len() - start;
                        self.last_tool_result = (!debug && count > 1).then_some((start, count));
                    }
                }
                MessageContent::Thinking(t) => {
                    if std::env::var("BIOROUTER_CLI_SHOW_THINKING").is_ok() {
                        self.push_line(Line::from(Span::styled(
                            "thinking".to_string(),
                            DIM.add_modifier(Modifier::ITALIC),
                        )));
                        for l in t.thinking.lines() {
                            self.push_line(Line::from(Span::styled(l.to_string(), DIM)));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Collapse the previous tool result's preview into a one-line summary so the
    /// conversation stays readable as tool calls pile up (only the current result
    /// stays expanded). Adjusts the live-stream anchor if it sits after the block.
    fn collapse_last_tool_result(&mut self) {
        let Some((start, len)) = self.last_tool_result.take() else {
            return;
        };
        if len < 2 || start + len > self.scrollback.len() {
            return;
        }
        let summary = Line::from(Span::styled(
            format!("    ⋯ {len} lines of tool output collapsed (scroll up to review)"),
            DIM,
        ));
        self.scrollback
            .splice(start..start + len, std::iter::once(summary));
        if let Some(s) = self.stream_start.as_mut() {
            if *s >= start + len {
                *s -= len - 1;
            }
        }
    }

    fn push_tool_header(&mut self, name: &str) {
        // A new tool call begins → the previous tool result is now "passed";
        // collapse it so only the current result stays expanded.
        self.collapse_last_tool_result();
        let parts: Vec<&str> = name.rsplit("__").collect();
        let tool = parts.first().copied().unwrap_or("tool");
        let ns = parts
            .split_first()
            .map(|(_, s)| s.iter().rev().copied().collect::<Vec<_>>().join("__"))
            .unwrap_or_default();
        self.push_blank();
        // An explicit, accented "tool call" badge so it's unmistakable that the
        // model is invoking a tool — simple but clearly indicative.
        let mut spans = vec![
            Span::styled(
                "▸ tool call ",
                Style::new()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                tool.to_string(),
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
        ];
        if !ns.is_empty() {
            spans.push(Span::styled(format!("  · {}", ns), DIM));
        }
        self.push_line(Line::from(spans));
    }

    // ── input editing ─────────────────────────────────────────────────────

    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.hist_idx = None;
        self.completion_dismissed = false;
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    /// Insert pasted text at the cursor as a single edit. Embedded newlines stay
    /// literal (they do NOT submit), so multi-line pastes compose correctly.
    pub fn paste(&mut self, text: &str) {
        let clean: String = text.replace('\r', "");
        self.input.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
        self.hist_idx = None;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self
            .input
            .get(..self.cursor)
            .and_then(|s| s.char_indices().next_back())
            .map(|(i, _)| i)
            .unwrap_or(0);
        self.input.replace_range(prev..self.cursor, "");
        self.cursor = prev;
        self.hist_idx = None;
        self.completion_dismissed = false;
    }

    pub fn move_left(&mut self) {
        if let Some((i, _)) = self
            .input
            .get(..self.cursor)
            .and_then(|s| s.char_indices().next_back())
        {
            self.cursor = i;
        }
    }

    pub fn move_right(&mut self) {
        if let Some(c) = self.input.get(self.cursor..).and_then(|s| s.chars().next()) {
            self.cursor += c.len_utf8();
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = self
            .input
            .get(..self.cursor)
            .and_then(|s| s.rfind('\n'))
            .map(|i| i + 1)
            .unwrap_or(0);
    }

    pub fn move_end(&mut self) {
        self.cursor = self
            .input
            .get(self.cursor..)
            .and_then(|s| s.find('\n'))
            .map(|i| self.cursor + i)
            .unwrap_or(self.input.len());
    }

    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.hist_idx = None;
        self.completion = None;
        self.completion_dismissed = false;
    }

    /// Replace the input with a value from history recall.
    pub fn set_input(&mut self, s: String) {
        self.cursor = s.len();
        self.input = s;
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = match self.hist_idx {
            Some(0) => 0,
            Some(i) => i - 1,
            None => self.history.len() - 1,
        };
        self.hist_idx = Some(idx);
        self.set_input(self.history[idx].clone());
    }

    pub fn history_next(&mut self) {
        match self.hist_idx {
            Some(i) if i + 1 < self.history.len() => {
                self.hist_idx = Some(i + 1);
                self.set_input(self.history[i + 1].clone());
            }
            Some(_) => {
                self.hist_idx = None;
                self.clear_input();
            }
            None => {}
        }
    }

    /// Inline "ghost" autofill: the dim remainder of the first slash command
    /// that the current input is a prefix of.
    pub fn ghost(&self) -> Option<&'static str> {
        let line = &self.input;
        if !line.starts_with('/') || line.contains(' ') || line.contains('\n') || line.len() < 2 {
            return None;
        }
        SLASH_COMMANDS
            .iter()
            .find(|c| c.starts_with(line.as_str()) && **c != line.as_str())
            .and_then(|c| c.get(line.len()..))
    }

    pub fn accept_ghost(&mut self) {
        if let Some(g) = self.ghost() {
            let g = g.to_string();
            self.input.push_str(&g);
            self.input.push(' ');
            self.cursor = self.input.len();
        }
    }

    // ── scrolling ───────────────────────────────────────────────────────────

    pub fn scroll_up(&mut self, n: u16) {
        let max = self.max_scroll();
        self.scroll = (self.scroll + n).min(max);
    }

    pub fn scroll_down(&mut self, n: u16) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    fn max_scroll(&self) -> u16 {
        (self.last_total_lines as u16).saturating_sub(self.last_viewport_h)
    }
}

// ── markdown-lite ───────────────────────────────────────────────────────────

/// Render a block of markdown text into styled lines. Supports headings,
/// bullet/numbered lists, fenced code blocks, and inline `**bold**` / `code`.
pub fn md_lines(text: &str) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let mut in_code = false;
    let rows: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < rows.len() {
        let raw = rows[i];
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if in_code {
            out.push(Line::from(Span::styled(
                format!("  {}", raw),
                Style::new().fg(Color::Indexed(108)),
            )));
            i += 1;
            continue;
        }
        // GFM table: a `|`-delimited header row, a `|---|` delimiter, then body
        // rows. Rendered as an aligned box-drawing table (like the GUI).
        if raw.contains('|') && i + 1 < rows.len() && is_table_delim(rows[i + 1]) {
            let mut end = i + 2;
            while end < rows.len() && rows[end].contains('|') && !rows[end].trim().is_empty() {
                end += 1;
            }
            let header = split_table_row(raw);
            let body: Vec<Vec<String>> =
                rows[i + 2..end].iter().map(|l| split_table_row(l)).collect();
            out.extend(render_table(&header, &body));
            i = end;
            continue;
        }
        if let Some(h) = trimmed
            .strip_prefix("### ")
            .or_else(|| trimmed.strip_prefix("## "))
            .or_else(|| trimmed.strip_prefix("# "))
        {
            out.push(Line::from(Span::styled(
                h.to_string(),
                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
            )));
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let mut spans = vec![Span::styled("  • ", Style::new().fg(ACCENT))];
            spans.extend(inline_spans(rest, Style::default()));
            out.push(Line::from(spans));
            i += 1;
            continue;
        }
        out.push(Line::from(inline_spans(raw, Style::default())));
        i += 1;
    }
    out
}

/// Split one `| a | b |` table row into trimmed cell strings.
fn split_table_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// True for a GFM delimiter row like `|---|:--:|` (only dashes, colons, pipes).
fn is_table_delim(line: &str) -> bool {
    let cells = split_table_row(line);
    !cells.is_empty()
        && cells.iter().all(|c| {
            let t = c.trim();
            !t.is_empty() && t.contains('-') && t.chars().all(|ch| ch == '-' || ch == ':')
        })
}

/// Render a parsed table as aligned box-drawing rows. Columns are sized to their
/// widest cell (capped) so the grid stays tidy; the header is accented.
fn render_table(header: &[String], body: &[Vec<String>]) -> Vec<Line<'static>> {
    let ncols = header
        .len()
        .max(body.iter().map(|r| r.len()).max().unwrap_or(0));
    if ncols == 0 {
        return Vec::new();
    }
    let mut widths = vec![0usize; ncols];
    for (c, h) in header.iter().enumerate() {
        widths[c] = widths[c].max(UnicodeWidthStr::width(h.as_str()));
    }
    for row in body {
        for (c, cell) in row.iter().enumerate() {
            if c < ncols {
                widths[c] = widths[c].max(UnicodeWidthStr::width(cell.as_str()));
            }
        }
    }
    for w in widths.iter_mut() {
        *w = (*w).clamp(1, 40);
    }

    let border = |left: &str, mid: &str, right: &str| -> Line<'static> {
        let mut s = String::from(left);
        for (c, w) in widths.iter().enumerate() {
            s.push_str(&"─".repeat(w + 2));
            s.push_str(if c + 1 < widths.len() { mid } else { right });
        }
        Line::from(Span::styled(s, DIM))
    };
    let pad = |s: &str, w: usize| -> String {
        let mut acc = String::new();
        let mut accw = 0usize;
        for ch in s.chars() {
            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
            if accw + cw > w {
                break;
            }
            acc.push(ch);
            accw += cw;
        }
        acc.push_str(&" ".repeat(w.saturating_sub(accw)));
        acc
    };
    let row_line = |cells: &[String], style: Style| -> Line<'static> {
        let mut spans = Vec::new();
        for (c, w) in widths.iter().enumerate() {
            spans.push(Span::styled("│ ", DIM));
            let cell = cells.get(c).map(|s| s.as_str()).unwrap_or("");
            spans.push(Span::styled(pad(cell, *w), style));
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled("│", DIM));
        Line::from(spans)
    };

    let mut out = Vec::new();
    out.push(border("┌", "┬", "┐"));
    out.push(row_line(
        header,
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
    ));
    out.push(border("├", "┼", "┤"));
    for row in body {
        out.push(row_line(row, Style::default()));
    }
    out.push(border("└", "┴", "┘"));
    out
}

/// Parse inline `**bold**` and `` `code` `` markers into styled spans.
fn inline_spans(s: &str, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            flush(&mut buf, base, &mut spans);
            let mut code = String::new();
            for cc in chars.by_ref() {
                if cc == '`' {
                    break;
                }
                code.push(cc);
            }
            spans.push(Span::styled(
                code,
                Style::new()
                    .fg(Color::Indexed(108))
                    .add_modifier(Modifier::DIM),
            ));
        } else if c == '*' && chars.peek() == Some(&'*') {
            chars.next();
            flush(&mut buf, base, &mut spans);
            let mut bold = String::new();
            while let Some(&cc) = chars.peek() {
                if cc == '*' {
                    chars.next();
                    if chars.peek() == Some(&'*') {
                        chars.next();
                        break;
                    }
                    bold.push('*');
                } else {
                    bold.push(cc);
                    chars.next();
                }
            }
            spans.push(Span::styled(bold, base.add_modifier(Modifier::BOLD)));
        } else {
            buf.push(c);
        }
    }
    flush(&mut buf, base, &mut spans);
    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }
    spans
}

fn flush(buf: &mut String, base: Style, spans: &mut Vec<Span<'static>>) {
    if !buf.is_empty() {
        spans.push(Span::styled(std::mem::take(buf), base));
    }
}

fn compact_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => truncate_lines(s, 1),
        other => other.to_string(),
    }
}

fn truncate_lines(s: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = s.lines().collect();
    if lines.len() <= max_lines {
        s.to_string()
    } else {
        format!(
            "{}\n… (+{} lines)",
            lines[..max_lines].join("\n"),
            lines.len() - max_lines
        )
    }
}
