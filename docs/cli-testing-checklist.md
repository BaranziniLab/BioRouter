# Biorouter CLI — Comprehensive Testing Checklist

A diverse, end-to-end checklist for exercising the `biorouter` CLI and verifying
parity with the desktop GUI. Status legend:

- **[verified]** — exercised headlessly in this pass and confirmed working.
- **[manual]** — requires a real interactive terminal (raw-mode TUI keystrokes)
  or a live provider; steps given, to be run by hand.
- **[audited]** — verified by code review + automated `TestBackend`/unit tests
  (cannot be driven headlessly).
- **[gap]** — known GUI-parity gap (not yet in the CLI).

Run from the repo root after `source bin/activate-hermit && cargo build -p biorouter-cli`.
Binary: `./target/debug/biorouter`. Force the classic readline UI with
`BIOROUTER_CLI_CLASSIC=1`; the full-screen TUI is the default on a TTY.

---

## 1. Launch modes & output formats

- [verified] `biorouter --version`, `biorouter --help`, every `<cmd> --help`.
- [audited] `biorouter session` → full-screen TUI (default on a TTY).
- [manual] `BIOROUTER_CLI_CLASSIC=1 biorouter session` → classic readline REPL.
- [verified] `biorouter run -t "<prompt>"` → headless prompting (one-shot).
- [manual] `echo "<prompt>" | biorouter run -i -` → instructions from stdin.
- [manual] `biorouter run -t "..." --output-format json` and `stream-json`.
- [verified] Non-TTY / piped invocation auto-falls back to the classic path.

## 2. Interactive TUI (run `biorouter session`)

Input & editing:
- [audited] Type text; **blinking bar (I-beam) cursor** sits at the insertion point.
- [audited] **CJK / wide chars** (e.g. `你好世界`) keep the caret at the true end.
- [audited] `Ctrl+J` inserts a newline; multi-line compose grows the input box (≤6 rows).
- [verified] **Paste** of multi-line text inserts as one chunk and does **not**
  submit prematurely (bracketed paste; unit-tested).
- [audited] `←/→/Home/End` move by character; `Backspace` deletes by character.
- [audited] `↑/↓` recall input history (single-line buffer).
- [audited] Slash **ghost autofill**: typing `/co` dims in `mpact`; `Tab` accepts.
- [audited] `Enter` submits a non-empty buffer; empty Enter is a no-op.

Layout & chrome:
- [audited] Greeting shows the coral-brown `BIOROUTER` wordmark, tagline, and
  **working directory** line.
- [audited] **Two-line status**: line 1 = model · provider; line 2 = N skills ·
  N extensions · N knowledge bases · context meter (right-aligned).
- [audited] **Context meter** updates after each turn; warns red ≥85%.
- [audited] ~2-row **gap** separates the response from the input box.
- [audited] Window starts compact and **grows downward** as the chat fills, then scrolls.
- [manual] Mouse wheel / PageUp / PageDown scroll the history; latest output stays visible.
- [manual] Terminal **resize** reflows without corruption.

Turns, tools, control:
- [verified] Send a prompt → streaming response renders; **thinking spinner** on the input border.
- [verified] Tool calls render as a distinct **`▸ tool call`** badge (tool + namespace);
  arguments in plain text (no green).
- [manual] **Permission modal**: a tool needing approval shows a modal; `↑/↓` + `Enter`
  pick Allow / Always allow / Deny / Cancel; `Esc`/`Ctrl+C` cancels.
- [manual] **Ctrl-C mid-stream** cancels the in-flight response promptly (event-channel fix).
- [audited] Markdown rendering: headings, **bold**, `inline code`, bullets, code fences.

Slash commands (TUI):
- [audited] `/help`, `/?` → in-TUI command + navigation help (now lists `/compact`).
- [verified-by-code] `/clear` clears the **persisted** conversation + token counts +
  scrollback (no desync; shared `clear_conversation`).
- [manual] `/compact` → condenses the conversation via a normal turn.
- [audited] `/exit`, `/quit` → leave; `Ctrl+C` on empty input quits.
- [audited] Other slash commands show a "use BIOROUTER_CLI_CLASSIC=1" note.
- [audited] **Panic safety**: a render panic restores raw mode / alt screen / cursor
  (panic hook) — terminal is never left corrupted; `Tui::Drop` covers the error path.

## 3. Classic REPL (`BIOROUTER_CLI_CLASSIC=1 biorouter session`)
- [manual] `/help`, `/t [light|dark|ansi]`, `/r`, `/mode <m>`, `/plan` … `/endplan`,
  `/compact`, `/clear`, `/workflow`, `/extension`, `/builtin`.
- [audited] Interrupt handling no longer panics on an empty last message.

## 4. Models / provider config  (parity: GUI settings → models/providers)
- [verified] `biorouter models current` — shows configured provider+model.
- [verified] `biorouter models providers` — lists providers + defaults.
- [verified] `biorouter models list <provider>` — known models; unknown → clear error.
- [manual] `biorouter models set --provider <p> --model <m>` — writes shared config.yaml.
- [manual] `biorouter configure` — interactive provider/key wizard.

## 5. Knowledge bases  (parity: GUI Knowledge view)
- [verified] `knowledge list` — visible (●) vs hidden (○) model, with a hint line.
- [verified] `knowledge active`, `knowledge active --set <id>`, `--clear`.
- [manual] `knowledge create <id> --name "<n>"` — creates + sets active when none.
- [verified] `knowledge hide <id>` / `knowledge unhide <id>` — controls agent visibility;
  unknown id → clear error.
- [verified] `knowledge query "<q>"` — live LLM answer over the active base (verified end-to-end).
- [manual] `knowledge ingest --url <u>` / `--file <p>` / `--text "<t>" [--focus ...]`.
- [manual] `knowledge lint [--fix]` — deterministic scan (verified clean) + optional autofix.
- [gap] No CLI for knowledge **graph**, **history/restore**, or **`.brkb` export/import**
  (GUI/server-only).

## 6. Extensions  (parity: GUI Extensions + .brxt install)
- [verified] `extension list` — configured extensions with enabled/disabled dots.
- [manual] `extension install <file.brxt> [--env K=V] [--secret K=V] [--no-enable]`
  — extracts to `~/.config/biorouter/extensions/<name>`, runs **`uv sync`**, registers a
  stdio extension; missing `uv` → actionable error; missing file → clear error (verified).
- [manual] `extension remove <name> [--purge]`.
- [manual] In a session: `/extension <ENV=v cmd args>`, `/builtin <names>`.

## 7. Skills  (parity: GUI Skills + .zip install)
- [verified] `skill list` — installed skills (incl. bundles).
- [verified] `skill install <file.zip> [--force]` — single skill **and** bundle layouts
  (verified install + remove round-trip); bad/missing zip → clear error.
- [verified] `skill remove <slug>`.

## 8. Workflows  (parity: GUI Workflows; builder is GUI-only)
- [verified] `workflow list` — local + GitHub workflows.
- [verified] `workflow install <file.json|yaml>` — validates + saves to the library;
  wrong extension → clear error.
- [manual] `workflow validate <name|path>`, `workflow deeplink <name>`, `workflow open <name>`.
- [manual] `biorouter run --workflow <name|path> [--explain]`.

## 9. Scheduler  (parity: GUI Schedules)
- [verified] `schedule list` (empty → "No scheduled jobs found"), `schedule cron-help`.
- [manual] `schedule add …`, `schedule run-now <id>`, `schedule sessions <id>`, `schedule remove <id>`.

## 10. Sessions
- [manual] `session --resume` / `--name` / `--session-id`; `session list`, `session remove`,
  `session export`, `session diagnostics`.

## 11. Misc commands
- [verified] `info`, `info --verbose` (brand now reads **"Biorouter"**).
- [verified] `completion <bash|zsh|fish|…>`.
- [manual] `project`, `projects` (interactive directory manager).
- [manual] `update [--canary]`, `web`, `term`, `mcp <server>`, `acp`.

## 12. Error handling & robustness (spot-checked)
- [verified] Unknown subcommand, missing files, unknown provider, missing `--kb`,
  wrong workflow extension → all produce clear, non-panicking errors (exit ≠ 0).

---

## Bugs found & fixed in this pass
1. `info` printed **"Biorouter"** → corrected to **"Biorouter"**.
2. TUI `/clear` only reset in-memory messages → now clears the **persisted** conversation
   + token counts (shared `CliSession::clear_conversation`), so the context meter and a
   reopened session stay correct.
3. TUI lost/delayed keystrokes (incl. mid-stream **Ctrl-C**) because multiple
   `EventStream::next()` futures raced across `select!` arms → a **single reader task now
   forwards events over an mpsc channel** (cancel-safe).
4. Multi-line **paste** fired line-by-line (premature submit) → **bracketed paste** enabled;
   pastes insert as one chunk.
5. No terminal restore on **panic** → installed a panic hook restoring raw mode / alt
   screen / cursor before the default hook.
6. Scroll height estimate counted chars, not display width → **wide (CJK) lines could clip**;
   now uses unicode width.
7. Tool-call permission **cancel** could desync the conversation → now drains the stream
   after cancelling.
8. Classic interrupt handler could **panic** on an empty last message → now a graceful warn.
9. `/compact` was unavailable in the TUI → now wired (sends the compaction trigger turn).

## Known GUI-parity gaps (not bugs — future work)
- Image/file attachment (multimodal input) — GUI-only.
- Voice dictation (Whisper) — GUI-only.
- Knowledge graph view, history/restore, `.brkb` export/import — server/GUI-only.
- First-class secrets-management command (keys only via `configure` / per-extension `--secret`).
- Dashboard canvas, interactive MCP Apps/MCP-UI, session-sharing tunnel, response-style
  settings — GUI-only.
</content>
