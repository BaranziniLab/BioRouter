//! Terminal markdown renderer for assistant output.
//!
//! Mirrors the GUI's `MarkdownContent.tsx` feature set as closely as a
//! terminal allows: headings, bold/italic/strikethrough, inline code, fenced
//! code blocks (syntax-highlighted via bat), tables, blockquotes (including
//! GitHub-style callouts), ordered/unordered/task lists, links, horizontal
//! rules, and TeX math passthrough.
//!
//! The renderer is a single pass over pulldown-cmark events that word-wraps
//! styled text to the terminal width. Styling goes through `console::style`,
//! which already honours NO_COLOR and non-TTY output.

use console::{measure_text_width, style};
use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

use super::output::{env_no_color, Theme, ACCENT};
use unicode_width::UnicodeWidthChar;

/// Split a (possibly ANSI-styled) string at the largest prefix whose *visible*
/// width is `<= max`, returning `(head, head_width, tail)`. ANSI escape
/// sequences carry zero width and are copied verbatim into whichever side the
/// scan is on, so styling survives the break. Used to hard-wrap words that are
/// individually wider than the terminal (long URLs, paths, hashes) instead of
/// letting them overflow and wrap into broken layout.
fn split_styled_at_width(s: &str, max: usize) -> (String, usize, String) {
    let mut head = String::new();
    let mut head_w = 0usize;
    let mut chars = s.char_indices().peekable();
    let mut split_at = s.len();

    while let Some(&(i, c)) = chars.peek() {
        if c == '\u{1b}' {
            // Copy a full escape sequence verbatim (zero visible width).
            // ESC [ <params> <final 0x40..=0x7E>, or ESC <single byte>.
            head.push(c);
            chars.next();
            if let Some(&(_, '[')) = chars.peek() {
                head.push('[');
                chars.next();
                while let Some(&(_, pc)) = chars.peek() {
                    head.push(pc);
                    chars.next();
                    if ('\u{40}'..='\u{7e}').contains(&pc) {
                        break;
                    }
                }
            } else if let Some(&(_, pc)) = chars.peek() {
                head.push(pc);
                chars.next();
            }
            continue;
        }
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
        if head_w + cw > max {
            split_at = i;
            break;
        }
        head.push(c);
        head_w += cw;
        chars.next();
    }

    // `split_at` is always a char boundary (a `char_indices` index or `len`).
    let tail = s.get(split_at..).unwrap_or("").to_string();
    (head, head_w, tail)
}

/// Heuristic: does this (possibly ANSI-styled) word look like a URL we should
/// keep intact rather than hard-wrap? Covers `scheme://…`, `www.…`, and the
/// `(https://…)` form the link renderer emits after the link text.
fn is_url_like(word: &str) -> bool {
    let bare = console::strip_ansi_codes(word);
    let bare = bare.trim_start_matches('(');
    bare.contains("://") || bare.starts_with("www.")
}

/// Bullet glyphs by nesting depth (cycles past the end).
const BULLETS: [&str; 3] = ["•", "◦", "▪"];

/// xterm-256 warm orange used for inline code — close to the GUI's warm
/// palette for `code` spans without touching the reserved brand accent.
const INLINE_CODE_COLOR: u8 = 173;

pub fn render_markdown(content: &str, theme: Theme, width: usize) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_GFM); // blockquote callouts (> [!NOTE] …)

    let mut r = Renderer::new(theme, width.max(20));
    for event in Parser::new_ext(content, options) {
        r.handle(event);
    }
    r.finish()
}

struct ListLevel {
    /// Next number for ordered lists, None for bullet lists.
    counter: Option<u64>,
    /// Continuation indent contributed by the current item at this level.
    indent: usize,
}

struct Renderer {
    theme: Theme,
    width: usize,
    out: String,

    // Current line and word being assembled (styled text + visible width).
    line: String,
    line_w: usize,
    line_has_content: bool,
    word: String,
    word_w: usize,

    // Inline style state.
    bold: bool,
    italic: bool,
    strike: bool,
    inline_code: bool,
    heading: Option<HeadingLevel>,
    link_dest: Option<String>,
    link_text: String,

    // Block state.
    quote_depth: usize,
    lists: Vec<ListLevel>,
    pending_marker: Option<(String, usize)>,
    /// Set right after opening a blockquote so the quote's first paragraph
    /// doesn't emit a leading `┃` separator line.
    suppress_para_sep: bool,

    // Fenced/indented code block accumulation.
    code_lang: Option<String>,
    code_buf: String,

    // Table accumulation.
    in_table: bool,
    in_table_head: bool,
    table_aligns: Vec<Alignment>,
    table_rows: Vec<Vec<(String, usize)>>,
    cur_row: Vec<(String, usize)>,
    cur_cell: String,
    cur_cell_w: usize,
    cell_has_content: bool,
}

impl Renderer {
    fn new(theme: Theme, width: usize) -> Self {
        Renderer {
            theme,
            width,
            out: String::new(),
            line: String::new(),
            line_w: 0,
            line_has_content: false,
            word: String::new(),
            word_w: 0,
            bold: false,
            italic: false,
            strike: false,
            inline_code: false,
            heading: None,
            link_dest: None,
            link_text: String::new(),
            quote_depth: 0,
            lists: Vec::new(),
            pending_marker: None,
            suppress_para_sep: false,
            code_lang: None,
            code_buf: String::new(),
            in_table: false,
            in_table_head: false,
            table_aligns: Vec::new(),
            table_rows: Vec::new(),
            cur_row: Vec::new(),
            cur_cell: String::new(),
            cur_cell_w: 0,
            cell_has_content: false,
        }
    }

    // ----- styled text plumbing -------------------------------------------

    fn style_run(&self, s: &str) -> String {
        let mut st = style(s.to_string());
        if self.inline_code {
            st = st.color256(INLINE_CODE_COLOR);
        }
        if let Some(level) = self.heading {
            st = st.bold();
            if matches!(level, HeadingLevel::H1 | HeadingLevel::H2) {
                st = st.fg(ACCENT);
            }
        }
        if self.bold {
            st = st.bold();
        }
        if self.italic {
            st = st.italic();
        }
        if self.strike {
            st = st.strikethrough();
        }
        if self.link_dest.is_some() {
            st = st.cyan().underlined();
        }
        if self.in_table_head {
            st = st.bold();
        }
        st.to_string()
    }

    /// Append text, splitting it into words for wrapping. Whitespace between
    /// events is preserved as word boundaries; adjacent non-whitespace runs
    /// (e.g. `**bold**.`) glue onto the same word.
    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let leading_ws = text.starts_with(char::is_whitespace);
        let trailing_ws = text.ends_with(char::is_whitespace);
        // Leading whitespace closes the previous word, so `… and` doesn't glue
        // onto a preceding styled fragment.
        if leading_ws {
            self.emit_word();
        }
        let mut any = false;
        for run in text.split_whitespace() {
            // Whitespace between words within this event is a word boundary.
            if any {
                self.emit_word();
            }
            any = true;
            if self.link_dest.is_some() {
                if !self.link_text.is_empty() {
                    self.link_text.push(' ');
                }
                self.link_text.push_str(run);
            }
            let styled = self.style_run(run);
            self.push_word_fragment(&styled, measure_text_width(run));
        }
        // Trailing whitespace ends the last word so the next fragment doesn't
        // glue onto it.
        if trailing_ws {
            self.emit_word();
        }
    }

    /// Append an already-styled fragment to the current word without
    /// introducing a word boundary.
    fn push_word_fragment(&mut self, styled: &str, visible: usize) {
        self.word.push_str(styled);
        self.word_w += visible;
    }

    /// Flush the current word into the line, wrapping if needed.
    fn emit_word(&mut self) {
        if self.word.is_empty() {
            return;
        }
        if self.in_table {
            if self.cell_has_content {
                self.cur_cell.push(' ');
                self.cur_cell_w += 1;
            }
            let word = std::mem::take(&mut self.word);
            self.cur_cell.push_str(&word);
            self.cur_cell_w += self.word_w;
            self.cell_has_content = true;
            self.word_w = 0;
            return;
        }
        let word = std::mem::take(&mut self.word);
        let word_w = self.word_w;
        self.word_w = 0;

        let sep = usize::from(self.line_has_content);
        // Fits on the current line as-is.
        if self.line_w + sep + word_w <= self.width {
            if self.line.is_empty() {
                self.start_line();
            }
            if self.line_has_content {
                self.line.push(' ');
                self.line_w += 1;
            }
            self.line.push_str(&word);
            self.line_w += word_w;
            self.line_has_content = true;
            return;
        }
        // Doesn't fit: move to a fresh line first.
        if self.line_has_content {
            self.write_line();
        }
        if self.line.is_empty() {
            self.start_line();
        }
        // Fits on the fresh line.
        if self.line_w + word_w <= self.width {
            self.line.push_str(&word);
            self.line_w += word_w;
            self.line_has_content = true;
            return;
        }
        // Still too wide even on its own line. URLs are left intact so they
        // stay clickable / copyable as a single unit (the terminal soft-wraps
        // them); any other unbreakable token (long hashes, paths, base64) is
        // hard-broken so it can't wrap into broken layout.
        if is_url_like(&word) {
            self.line.push_str(&word);
            self.line_w += word_w;
            self.line_has_content = true;
            return;
        }
        let mut rest = word;
        loop {
            if self.line.is_empty() {
                self.start_line();
            }
            let budget = self.width.saturating_sub(self.line_w).max(1);
            let (head, head_w, tail) = split_styled_at_width(&rest, budget);
            // Degenerate guard: if nothing was consumed (budget too small for
            // even one glyph), emit the remainder whole rather than spin.
            if head.is_empty() {
                self.line.push_str(&rest);
                self.line_w += measure_text_width(&rest);
                self.line_has_content = true;
                break;
            }
            self.line.push_str(&head);
            self.line_w += head_w;
            self.line_has_content = true;
            if tail.is_empty() {
                break;
            }
            self.write_line();
            rest = tail;
        }
    }

    /// Build the prefix for a fresh line: blockquote bars, list indentation,
    /// and a pending list marker if one is queued.
    fn start_line(&mut self) {
        let mut prefix = String::new();
        let mut pw = 0;
        for _ in 0..self.quote_depth {
            prefix.push_str(&style("┃ ").dim().to_string());
            pw += 2;
        }
        if let Some((marker, mw)) = self.pending_marker.take() {
            let base: usize = self
                .lists
                .iter()
                .take(self.lists.len().saturating_sub(1))
                .map(|l| l.indent)
                .sum();
            prefix.push_str(&" ".repeat(base));
            prefix.push_str(&marker);
            prefix.push(' ');
            pw += base + mw + 1;
        } else {
            let total: usize = self.lists.iter().map(|l| l.indent).sum();
            prefix.push_str(&" ".repeat(total));
            pw += total;
        }
        self.line = prefix;
        self.line_w = pw;
        self.line_has_content = false;
    }

    /// Write out the current line (if it has any content) and reset it.
    fn write_line(&mut self) {
        if self.line_has_content {
            let line = std::mem::take(&mut self.line);
            self.out.push_str(line.trim_end_matches(' '));
            self.out.push('\n');
        }
        self.line.clear();
        self.line_w = 0;
        self.line_has_content = false;
    }

    fn flush(&mut self) {
        self.emit_word();
        self.write_line();
    }

    /// Separate top-level blocks with a blank line (a `┃` line inside
    /// blockquotes). Inside lists items stay tight.
    fn para_sep(&mut self) {
        self.flush();
        if self.suppress_para_sep {
            self.suppress_para_sep = false;
            return;
        }
        if self.out.is_empty() {
            return;
        }
        if !self.lists.is_empty() {
            return;
        }
        if self.quote_depth > 0 {
            let mut prefix = String::new();
            for _ in 0..self.quote_depth {
                prefix.push_str(&style("┃ ").dim().to_string());
            }
            self.out.push_str(prefix.trim_end_matches(' '));
            self.out.push('\n');
        } else if !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    /// Emit a full-width dim rule directly to the output.
    fn emit_rule(&mut self) {
        self.flush();
        self.out
            .push_str(&style("─".repeat(self.width)).dim().to_string());
        self.out.push('\n');
    }

    // ----- code blocks -----------------------------------------------------

    fn emit_code_block(&mut self) {
        let lang = self.code_lang.take().unwrap_or_default();
        let code = std::mem::take(&mut self.code_buf);
        let code_trimmed = code.trim_end_matches('\n');

        // Header rule with the language label, e.g. "── rust ────────".
        let label = if lang.is_empty() {
            "─".repeat(self.width)
        } else {
            let head = format!("── {} ", lang);
            let fill = self.width.saturating_sub(measure_text_width(&head));
            format!("{}{}", head, "─".repeat(fill))
        };
        self.out.push_str(&style(label).dim().to_string());
        self.out.push('\n');

        let highlighted = highlight_code(code_trimmed, &lang, self.theme);
        for line in highlighted.lines() {
            self.out.push_str("  ");
            self.out.push_str(line);
            self.out.push('\n');
        }

        self.out
            .push_str(&style("─".repeat(self.width)).dim().to_string());
        self.out.push('\n');
    }

    // ----- tables -----------------------------------------------------------

    fn emit_table(&mut self) {
        let rows = std::mem::take(&mut self.table_rows);
        let aligns = std::mem::take(&mut self.table_aligns);
        if rows.is_empty() {
            return;
        }
        let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if ncols == 0 {
            return;
        }

        // Natural column widths, then shrink the widest columns until the
        // table fits the terminal.
        let mut colw = vec![1usize; ncols];
        for row in &rows {
            for (i, (_, w)) in row.iter().enumerate() {
                colw[i] = colw[i].max(*w);
            }
        }
        let total = |colw: &[usize]| colw.iter().sum::<usize>() + 3 * colw.len() + 1;
        // Shrink the widest column repeatedly until the table fits the terminal.
        // Columns may be squeezed all the way down to a single character: a
        // heavily-truncated cell is far better than a table whose borders are
        // wider than the terminal and wrap into garbage. The shrink targets the
        // widest column each step so widths stay as balanced (and readable) as
        // the available space allows.
        let mut guard = 0;
        while total(&colw) > self.width && guard < 10_000 {
            guard += 1;
            if let Some((idx, _)) = colw
                .iter()
                .enumerate()
                .filter(|(_, w)| **w > 1)
                .max_by_key(|(_, w)| **w)
            {
                colw[idx] -= 1;
            } else {
                break;
            }
        }

        let dim = |s: String| style(s).dim().to_string();
        let border = |left: &str, mid: &str, right: &str| -> String {
            let segs: Vec<String> = colw.iter().map(|w| "─".repeat(w + 2)).collect();
            dim(format!("{}{}{}", left, segs.join(mid), right))
        };

        self.out.push_str(&border("╭", "┬", "╮"));
        self.out.push('\n');
        for (ri, row) in rows.iter().enumerate() {
            let mut line = String::new();
            line.push_str(&dim("│".to_string()));
            for (ci, &col_max) in colw.iter().enumerate() {
                let (content, cw) = row
                    .get(ci)
                    .map(|(c, w)| (c.as_str(), *w))
                    .unwrap_or(("", 0));
                let (content, cw) = if cw > col_max {
                    let truncated = console::truncate_str(content, col_max, "…").to_string();
                    let tw = measure_text_width(&truncated);
                    (std::borrow::Cow::Owned(truncated), tw)
                } else {
                    (std::borrow::Cow::Borrowed(content), cw)
                };
                let pad = col_max.saturating_sub(cw);
                let (lpad, rpad) = match aligns.get(ci) {
                    Some(Alignment::Right) => (pad, 0),
                    Some(Alignment::Center) => (pad / 2, pad - pad / 2),
                    _ => (0, pad),
                };
                line.push(' ');
                line.push_str(&" ".repeat(lpad));
                line.push_str(&content);
                line.push_str(&" ".repeat(rpad));
                line.push(' ');
                line.push_str(&dim("│".to_string()));
            }
            self.out.push_str(&line);
            self.out.push('\n');
            if ri == 0 && rows.len() > 1 {
                self.out.push_str(&border("├", "┼", "┤"));
                self.out.push('\n');
            }
        }
        self.out.push_str(&border("╰", "┴", "╯"));
        self.out.push('\n');
    }

    // ----- event dispatch ---------------------------------------------------

    fn handle(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start_tag(tag),
            Event::End(tag) => self.end_tag(tag),
            Event::Text(text) => {
                if self.code_lang.is_some() {
                    self.code_buf.push_str(&text);
                } else {
                    self.push_text(&text);
                }
            }
            Event::Code(code) => {
                self.inline_code = true;
                self.push_text(&code);
                self.inline_code = false;
            }
            Event::InlineMath(tex) => {
                let styled = style(format!("${}$", tex)).italic().to_string();
                let w = measure_text_width(&format!("${}$", tex));
                self.push_word_fragment(&styled, w);
                self.emit_word();
            }
            Event::DisplayMath(tex) => {
                self.para_sep();
                self.italic = true;
                self.push_text(&tex);
                self.italic = false;
                self.flush();
            }
            Event::SoftBreak | Event::HardBreak => {
                // Match the GUI (remark-breaks): soft breaks render as real
                // line breaks, so pre-shaped text keeps its shape.
                self.flush();
            }
            Event::Rule => {
                self.para_sep();
                self.emit_rule();
            }
            Event::TaskListMarker(checked) => {
                let (glyph, styled) = if checked {
                    ("☑", style("☑".to_string()).green().to_string())
                } else {
                    ("☐", style("☐".to_string()).to_string())
                };
                // GitHub-style: the checkbox replaces the list bullet.
                if self.pending_marker.is_some() {
                    self.pending_marker = Some((styled, measure_text_width(glyph)));
                } else {
                    self.push_word_fragment(&styled, measure_text_width(glyph));
                    self.emit_word();
                }
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                // No HTML rendering in a terminal — show it dimmed.
                let trimmed = html.trim_end_matches('\n');
                if !trimmed.is_empty() {
                    let styled = style(trimmed.to_string()).dim().to_string();
                    self.push_word_fragment(&styled, measure_text_width(trimmed));
                    self.emit_word();
                }
            }
            Event::FootnoteReference(name) => {
                let txt = format!("[^{}]", name);
                let styled = style(txt.clone()).dim().to_string();
                self.push_word_fragment(&styled, measure_text_width(&txt));
                self.emit_word();
            }
        }
    }

    fn start_tag(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                if self.lists.is_empty() {
                    self.para_sep();
                } else {
                    self.flush();
                }
            }
            Tag::Heading { level, .. } => {
                self.para_sep();
                self.heading = Some(level);
            }
            Tag::BlockQuote(kind) => {
                self.para_sep();
                self.quote_depth += 1;
                self.suppress_para_sep = true;
                if let Some(kind) = kind {
                    let (label, glyph) = match kind {
                        BlockQuoteKind::Note => ("Note", "ℹ"),
                        BlockQuoteKind::Tip => ("Tip", "✦"),
                        BlockQuoteKind::Important => ("Important", "❗"),
                        BlockQuoteKind::Warning => ("Warning", "⚠"),
                        BlockQuoteKind::Caution => ("Caution", "⛔"),
                    };
                    let styled = match kind {
                        BlockQuoteKind::Note => style(format!("{} {}", glyph, label)).cyan(),
                        BlockQuoteKind::Tip => style(format!("{} {}", glyph, label)).green(),
                        BlockQuoteKind::Important => {
                            style(format!("{} {}", glyph, label)).magenta()
                        }
                        BlockQuoteKind::Warning => style(format!("{} {}", glyph, label)).yellow(),
                        BlockQuoteKind::Caution => style(format!("{} {}", glyph, label)).red(),
                    }
                    .bold()
                    .to_string();
                    let w = measure_text_width(&format!("{} {}", glyph, label));
                    self.push_word_fragment(&styled, w);
                    self.flush();
                }
            }
            Tag::CodeBlock(kind) => {
                self.para_sep();
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => lang.trim().to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.code_lang = Some(lang);
                self.code_buf.clear();
            }
            Tag::List(start) => {
                if self.lists.is_empty() {
                    self.para_sep();
                } else {
                    self.flush();
                }
                self.lists.push(ListLevel {
                    counter: start,
                    indent: 2,
                });
            }
            Tag::Item => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                let (marker, mw) = if let Some(level) = self.lists.last_mut() {
                    match level.counter {
                        Some(n) => {
                            level.counter = Some(n + 1);
                            let m = format!("{}.", n);
                            let w = measure_text_width(&m);
                            level.indent = w + 1;
                            (style(m).dim().to_string(), w)
                        }
                        None => {
                            let bullet = BULLETS[depth % BULLETS.len()];
                            level.indent = 2;
                            (
                                style(bullet.to_string()).fg(ACCENT).to_string(),
                                measure_text_width(bullet),
                            )
                        }
                    }
                } else {
                    ("•".to_string(), 1)
                };
                self.pending_marker = Some((marker, mw));
            }
            Tag::Emphasis => self.italic = true,
            Tag::Strong => self.bold = true,
            Tag::Strikethrough => self.strike = true,
            Tag::Link { dest_url, .. } => {
                self.link_dest = Some(dest_url.to_string());
                self.link_text.clear();
            }
            Tag::Image { dest_url, .. } => {
                let txt = format!("[image: {}]", dest_url);
                let styled = style(txt.clone()).dim().to_string();
                self.push_word_fragment(&styled, measure_text_width(&txt));
                self.emit_word();
            }
            Tag::Table(aligns) => {
                self.para_sep();
                self.in_table = true;
                self.table_aligns = aligns;
                self.table_rows.clear();
            }
            Tag::TableHead => {
                self.in_table_head = true;
                self.cur_row.clear();
            }
            Tag::TableRow => self.cur_row.clear(),
            Tag::TableCell => {
                self.cur_cell.clear();
                self.cur_cell_w = 0;
                self.cell_has_content = false;
            }
            _ => {}
        }
    }

    fn end_tag(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(level) => {
                self.flush();
                // Underline H1s with a rule so top-level sections stand out
                // the way large heading type does in the GUI.
                if level == HeadingLevel::H1 {
                    self.out
                        .push_str(&style("─".repeat(self.width)).fg(ACCENT).dim().to_string());
                    self.out.push('\n');
                }
                self.heading = None;
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
            }
            TagEnd::CodeBlock => self.emit_code_block(),
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Item => {
                // An empty item still shows its marker.
                if self.pending_marker.is_some() && self.word.is_empty() {
                    self.start_line();
                    self.line_has_content = true;
                }
                self.flush();
            }
            TagEnd::Emphasis => self.italic = false,
            TagEnd::Strong => self.bold = false,
            TagEnd::Strikethrough => self.strike = false,
            TagEnd::Link => {
                if let Some(dest) = self.link_dest.take() {
                    // Append "(url)" unless the text already is the URL
                    // (autolinks) or it's an internal anchor.
                    let text = std::mem::take(&mut self.link_text);
                    if text != dest && !dest.starts_with('#') {
                        self.emit_word();
                        let txt = format!("({})", dest);
                        let styled = style(txt.clone()).dim().to_string();
                        self.push_word_fragment(&styled, measure_text_width(&txt));
                    }
                }
            }
            TagEnd::Table => {
                self.in_table = false;
                self.emit_table();
            }
            TagEnd::TableHead => {
                self.in_table_head = false;
                let row = std::mem::take(&mut self.cur_row);
                self.table_rows.push(row);
            }
            TagEnd::TableRow => {
                let row = std::mem::take(&mut self.cur_row);
                self.table_rows.push(row);
            }
            TagEnd::TableCell => {
                self.emit_word();
                let cell = std::mem::take(&mut self.cur_cell);
                self.cur_row.push((cell, self.cur_cell_w));
                self.cur_cell_w = 0;
                self.cell_has_content = false;
            }
            _ => {}
        }
    }

    fn finish(mut self) -> String {
        self.flush();
        let trimmed = self.out.trim_end_matches('\n');
        let mut result = trimmed.to_string();
        if !result.is_empty() {
            result.push('\n');
        }
        result
    }
}

/// Syntax-highlight a code block with bat, falling back to the raw code when
/// the language is unknown or highlighting fails.
fn highlight_code(code: &str, lang: &str, theme: Theme) -> String {
    let mut buf = String::new();
    {
        let mut printer = bat::PrettyPrinter::new();
        printer
            .input(bat::Input::from_bytes(code.as_bytes()))
            .theme(theme.as_str())
            .colored_output(env_no_color() && console::colors_enabled())
            .wrapping_mode(bat::WrappingMode::NoWrapping(true));
        if !lang.is_empty() {
            printer.language(lang);
        }
        if printer.print_with_writer(Some(&mut buf)).is_err() {
            return code.to_string();
        }
    }
    if buf.is_empty() {
        code.to_string()
    } else {
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn plain(content: &str) -> String {
        plain_w(content, 80)
    }

    fn plain_w(content: &str, width: usize) -> String {
        console::set_colors_enabled(false);
        render_markdown(content, Theme::Ansi, width)
    }

    // ----- headings ---------------------------------------------------------

    #[test]
    #[serial]
    fn h1_renders_text_with_underline_rule() {
        let out = plain("# Title");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "Title");
        assert!(lines[1].chars().all(|c| c == '─'));
        assert_eq!(lines[1].chars().count(), 80);
    }

    #[test]
    #[serial]
    fn h2_and_h3_render_without_hash_markers() {
        let out = plain("## Section\n\n### Sub");
        assert!(out.contains("Section"));
        assert!(out.contains("Sub"));
        assert!(!out.contains('#'));
    }

    #[test]
    #[serial]
    fn h1_is_styled_bold_accent_when_colors_on() {
        console::set_colors_enabled(true);
        let out = render_markdown("# Title", Theme::Ansi, 80);
        console::set_colors_enabled(false);
        assert!(out.contains("\u{1b}[1m"), "expected bold ANSI: {:?}", out);
    }

    // ----- emphasis ---------------------------------------------------------

    #[test]
    #[serial]
    fn bold_italic_strike_markers_are_consumed() {
        let out = plain("**bold** *italic* ~~gone~~ normal");
        assert_eq!(out.trim(), "bold italic gone normal");
    }

    #[test]
    #[serial]
    fn bold_emits_ansi_bold() {
        console::set_colors_enabled(true);
        let out = render_markdown("**bold**", Theme::Ansi, 80);
        console::set_colors_enabled(false);
        assert!(out.contains("\u{1b}[1m"));
    }

    #[test]
    #[serial]
    fn strikethrough_emits_ansi_strike() {
        console::set_colors_enabled(true);
        let out = render_markdown("~~struck~~", Theme::Ansi, 80);
        console::set_colors_enabled(false);
        assert!(out.contains("\u{1b}[9m"));
    }

    #[test]
    #[serial]
    fn punctuation_glues_to_styled_words() {
        let out = plain("This is **important**.");
        assert_eq!(out.trim(), "This is important.");
    }

    // ----- inline code ------------------------------------------------------

    #[test]
    #[serial]
    fn inline_code_backticks_are_consumed() {
        let out = plain("run `cargo test` now");
        assert_eq!(out.trim(), "run cargo test now");
    }

    #[test]
    #[serial]
    fn inline_code_is_colored() {
        console::set_colors_enabled(true);
        let out = render_markdown("`code`", Theme::Ansi, 80);
        console::set_colors_enabled(false);
        assert!(out.contains("\u{1b}[38;5;173m"), "got: {:?}", out);
    }

    // ----- code blocks ------------------------------------------------------

    #[test]
    #[serial]
    fn fenced_code_block_keeps_content_and_shows_language() {
        let out = plain("```rust\nfn main() {}\n```");
        assert!(out.contains("rust"), "language label missing: {}", out);
        assert!(out.contains("fn main() {}"));
        // Rules above and below.
        assert!(
            out.lines()
                .filter(|l| l.starts_with('─') || l.starts_with("──"))
                .count()
                >= 2
        );
    }

    #[test]
    #[serial]
    fn code_block_preserves_blank_and_indented_lines() {
        let out = plain("```\nline1\n\n    indented\n```");
        assert!(out.contains("  line1"));
        assert!(out.contains("      indented"));
    }

    #[test]
    #[serial]
    fn code_block_with_unknown_language_falls_back_to_plain() {
        let out = plain("```nosuchlang\nhello world\n```");
        assert!(out.contains("hello world"));
    }

    #[test]
    #[serial]
    fn code_block_is_not_word_wrapped() {
        let long = "let x = 1; ".repeat(20);
        let out = plain_w(&format!("```\n{}\n```", long), 40);
        assert!(out.contains(long.trim_end()));
    }

    // ----- lists ------------------------------------------------------------

    #[test]
    #[serial]
    fn unordered_list_uses_bullets() {
        let out = plain("- one\n- two");
        assert!(out.contains("• one"));
        assert!(out.contains("• two"));
    }

    #[test]
    #[serial]
    fn ordered_list_uses_numbers_and_custom_start() {
        let out = plain("3. three\n4. four");
        assert!(out.contains("3. three"));
        assert!(out.contains("4. four"));
    }

    #[test]
    #[serial]
    fn nested_lists_indent_and_change_bullet() {
        let out = plain("- outer\n  - inner\n    - deepest");
        assert!(out.contains("• outer"));
        assert!(out.contains("  ◦ inner"));
        assert!(out.contains("    ▪ deepest"));
    }

    #[test]
    #[serial]
    fn list_continuation_lines_are_indented() {
        let out = plain_w("- this is a rather long list item that needs to wrap", 24);
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() > 1);
        assert!(lines[0].starts_with("• "));
        for cont in &lines[1..] {
            assert!(
                cont.starts_with("  "),
                "continuation not indented: {:?}",
                cont
            );
        }
    }

    #[test]
    #[serial]
    fn task_list_renders_checkboxes() {
        let out = plain("- [x] done\n- [ ] todo");
        assert!(out.contains("☑ done"), "got: {}", out);
        assert!(out.contains("☐ todo"));
    }

    // ----- blockquotes ------------------------------------------------------

    #[test]
    #[serial]
    fn blockquote_gets_bar_prefix() {
        let out = plain("> quoted text");
        assert!(out.contains("┃ quoted text"), "got: {}", out);
    }

    #[test]
    #[serial]
    fn nested_blockquote_gets_double_bar() {
        let out = plain("> outer\n>> inner");
        assert!(out.contains("┃ ┃ inner"), "got: {}", out);
    }

    #[test]
    #[serial]
    fn blockquote_wrap_keeps_bar_on_continuations() {
        let out = plain_w("> a quote that is long enough that it must wrap lines", 24);
        for line in out.lines() {
            assert!(line.starts_with('┃'), "line lost quote bar: {:?}", line);
        }
    }

    #[test]
    #[serial]
    fn github_callout_renders_label() {
        let out = plain("> [!NOTE]\n> useful info");
        assert!(out.contains("Note"), "got: {}", out);
        assert!(out.contains("┃ useful info"));
    }

    // ----- tables -----------------------------------------------------------

    #[test]
    #[serial]
    fn table_renders_box_borders_and_cells() {
        let out = plain("| Name | Value |\n|------|-------|\n| foo | 1 |\n| barbaz | 22 |");
        assert!(out.contains('╭'));
        assert!(out.contains('╰'));
        assert!(out.contains("│ Name"), "got: {}", out);
        assert!(out.contains("│ foo"));
        assert!(out.contains("│ barbaz"));
        // Header separator present.
        assert!(out.contains('┼'));
    }

    #[test]
    #[serial]
    fn table_columns_align_consistently() {
        let out = plain("| A | B |\n|---|---|\n| x | y |\n| longer | z |");
        let bars: Vec<usize> = out
            .lines()
            .filter(|l| l.starts_with('│'))
            .map(|l| l.chars().count())
            .collect();
        assert!(bars.len() >= 3);
        assert!(
            bars.windows(2).all(|w| w[0] == w[1]),
            "ragged table: {}",
            out
        );
    }

    #[test]
    #[serial]
    fn table_right_alignment_pads_left() {
        let out = plain("| N |\n|--:|\n| 7 |");
        let row = out.lines().find(|l| l.contains('7')).unwrap();
        assert!(row.contains(" 7 │"), "got: {}", row);
    }

    #[test]
    #[serial]
    fn wide_table_shrinks_to_terminal_width() {
        let wide = format!(
            "| {} | {} |\n|---|---|\n| {} | {} |",
            "x".repeat(60),
            "y".repeat(60),
            "a".repeat(60),
            "b".repeat(60)
        );
        let out = plain_w(&wide, 40);
        for line in out.lines() {
            assert!(
                measure_text_width(line) <= 40,
                "table line wider than terminal: {:?}",
                line
            );
        }
        assert!(out.contains('…'), "long cells should be ellipsised");
    }

    #[test]
    #[serial]
    fn table_with_inline_styles_in_cells() {
        let out = plain("| H |\n|---|\n| **bold** `code` |");
        assert!(out.contains("bold code"), "got: {}", out);
    }

    // ----- links ------------------------------------------------------------

    #[test]
    #[serial]
    fn link_shows_text_and_url() {
        let out = plain("see [the docs](https://example.com) here");
        assert!(out.contains("the docs"));
        assert!(out.contains("(https://example.com)"));
    }

    #[test]
    #[serial]
    fn autolink_does_not_duplicate_url() {
        let out = plain("<https://example.com>");
        assert_eq!(
            out.matches("https://example.com").count(),
            1,
            "got: {}",
            out
        );
    }

    #[test]
    #[serial]
    fn image_renders_placeholder() {
        let out = plain("![alt text](https://example.com/x.png)");
        assert!(out.contains("[image: https://example.com/x.png]"));
    }

    // ----- rules, breaks, paragraphs -----------------------------------------

    #[test]
    #[serial]
    fn horizontal_rule_renders_full_width_line() {
        let out = plain_w("above\n\n---\n\nbelow", 30);
        assert!(out
            .lines()
            .any(|l| l.chars().count() == 30 && l.chars().all(|c| c == '─')));
    }

    #[test]
    #[serial]
    fn paragraphs_are_separated_by_blank_line() {
        let out = plain("first para\n\nsecond para");
        assert_eq!(out, "first para\n\nsecond para\n");
    }

    #[test]
    #[serial]
    fn soft_breaks_preserve_line_shape() {
        // Matches the GUI's remark-breaks behavior: single newlines stay.
        let out = plain("line one\nline two");
        assert_eq!(out, "line one\nline two\n");
    }

    #[test]
    #[serial]
    fn long_paragraph_wraps_to_width() {
        let out = plain_w(
            "the quick brown fox jumps over the lazy dog again and again and again",
            20,
        );
        for line in out.lines() {
            assert!(measure_text_width(line) <= 20, "too wide: {:?}", line);
        }
        assert!(out.lines().count() > 1);
    }

    #[test]
    #[serial]
    fn unbreakable_url_longer_than_width_is_kept() {
        // URLs stay intact (one clickable/copyable unit) even when over-width.
        let url = "https://example.com/very/long/path/that/exceeds/width";
        let out = plain_w(url, 20);
        assert!(out.contains(url), "got: {:?}", out);
    }

    #[test]
    #[serial]
    fn unbreakable_non_url_token_is_hard_broken_to_width() {
        // A long non-URL token (e.g. a hash) must hard-wrap so no line overflows.
        let token = "a".repeat(120);
        let out = plain_w(&token, 20);
        for line in out.lines() {
            assert!(
                measure_text_width(line) <= 20,
                "line wider than terminal: {:?}",
                line
            );
        }
        assert!(out.lines().count() >= 6, "should span several lines: {:?}", out);
        // The content survives the break (concatenating the lines restores it).
        let joined: String = out.lines().collect();
        assert_eq!(joined, token);
    }

    #[test]
    #[serial]
    fn hard_break_preserves_inline_style_across_lines() {
        // A long styled (code) token must hard-break without leaking raw ANSI
        // onto the visible width.
        console::set_colors_enabled(false);
        let out = render_markdown(&format!("`{}`", "x".repeat(60)), Theme::Ansi, 20);
        for line in out.lines() {
            assert!(measure_text_width(line) <= 20, "too wide: {:?}", line);
        }
    }

    #[test]
    #[serial]
    fn many_column_table_fits_narrow_terminal() {
        // Regression: an 8-column table in a 40-col terminal used to overflow
        // (min column width floor of 4) and wrap its borders into garbage.
        let header = "| c1 | c2 | c3 | c4 | c5 | c6 | c7 | c8 |";
        let sep = "|----|----|----|----|----|----|----|----|";
        let row = "| alpha | beta | gamma | delta | epsilon | zeta | eta | theta |";
        let out = plain_w(&format!("{header}\n{sep}\n{row}"), 40);
        for line in out.lines() {
            assert!(
                measure_text_width(line) <= 40,
                "table line wider than terminal: {:?} ({})",
                line,
                measure_text_width(line)
            );
        }
        // All table border/content lines share one width (borders line up).
        let widths: Vec<usize> = out
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                t.starts_with('╭') || t.starts_with('│') || t.starts_with('├') || t.starts_with('╰')
            })
            .map(measure_text_width)
            .collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]), "ragged table: {widths:?}");
    }

    #[test]
    #[serial]
    fn split_styled_at_width_counts_visible_width_only() {
        let styled = format!("{}", style("hello").bold());
        let (head, w, tail) = split_styled_at_width(&styled, 3);
        assert_eq!(w, 3, "visible width should be 3");
        assert_eq!(measure_text_width(&head), 3);
        // Concatenating head+tail loses no visible characters.
        let round = format!("{head}{tail}");
        assert_eq!(console::strip_ansi_codes(&round), "hello");
    }

    // ----- regressions found via tmux PTY debugging ---------------------------

    #[test]
    #[serial]
    fn link_url_is_separated_from_text_by_space() {
        let out = plain("[the docs](https://example.com)");
        assert!(
            out.contains("the docs (https://example.com)"),
            "got: {}",
            out
        );
    }

    #[test]
    #[serial]
    fn blockquote_has_no_leading_empty_bar_line() {
        let out = plain("> quoted");
        assert_eq!(out, "┃ quoted\n", "got: {:?}", out);
    }

    #[test]
    #[serial]
    fn task_list_checkbox_replaces_bullet() {
        let out = plain("- [x] done");
        assert!(
            out.starts_with("☑ done"),
            "bullet should be replaced: {:?}",
            out
        );
        assert!(!out.contains('•'));
    }

    // ----- math / html / misc -------------------------------------------------

    #[test]
    #[serial]
    fn inline_math_passes_through_tex() {
        let out = plain("Euler: $e^{i\\pi} + 1 = 0$");
        assert!(out.contains("$e^{i\\pi} + 1 = 0$"), "got: {}", out);
    }

    #[test]
    #[serial]
    fn inline_html_is_shown_verbatim() {
        let out = plain("a <br> b");
        assert!(out.contains("<br>"));
    }

    #[test]
    #[serial]
    fn empty_input_renders_empty() {
        assert_eq!(plain(""), "");
    }

    #[test]
    #[serial]
    fn plain_text_is_untouched() {
        let out = plain("just a plain sentence.");
        assert_eq!(out, "just a plain sentence.\n");
    }

    #[test]
    #[serial]
    fn mixed_document_renders_every_block_type() {
        let doc = "# Report\n\nIntro with **bold**, `code`, and [a link](https://x.dev).\n\n\
                   ## Data\n\n| K | V |\n|---|---|\n| a | 1 |\n\n\
                   > [!WARNING]\n> careful\n\n\
                   - [x] step one\n- [ ] step two\n\n\
                   ```python\nprint('hi')\n```\n\n---\n\ndone";
        let out = plain(doc);
        assert!(out.contains("Report"));
        assert!(out.contains("bold"));
        assert!(out.contains("(https://x.dev)"));
        assert!(out.contains("│ K"));
        assert!(out.contains("Warning"));
        assert!(out.contains("☑ step one"));
        assert!(out.contains("print('hi')"));
        assert!(out.contains("done"));
        assert!(!out.contains("**"), "raw bold markers leaked: {}", out);
        assert!(!out.contains("|---"), "raw table syntax leaked");
    }
}
