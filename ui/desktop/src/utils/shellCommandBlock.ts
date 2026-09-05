/**
 * Which chat code blocks may be run in the in-app terminal, and what bytes a
 * click on Run actually sends.
 *
 * Both halves are pure and live together because they are two answers to one
 * question — "is this a command, and what would typing it look like?" — and
 * getting either wrong writes the wrong thing into a real shell.
 */

/**
 * Fence identifiers whose contents are a shell COMMAND.
 *
 * Exact-match against the FULL fence identifier, which is why the caller must
 * pass the whole token rather than `MarkdownCode`'s display language: that one
 * comes from `/language-(\w+)/`, and `\w` stops at the hyphen, so a
 * ```shell-session fence arrives there as the string `shell`. An allowlist
 * consulted with the truncated token would hand a Run button to every terminal
 * TRANSCRIPT in the conversation.
 *
 * Transcripts are the deliberate exclusion, and the reason is not stylistic:
 * `console`, `shell-session`, `sh-session` and `terminal` conventionally hold
 * `$ cmd` followed by that command's OUTPUT, so running one verbatim executes
 * the prompt character and then several lines of prose. `text`, `markdown` and
 * `diff` are excluded for the same reason — they are not commands.
 *
 * Measured against this repository's own corpus on 2026-09-05 (every fence in
 * *.md/*.rs/*.ts/*.tsx, node_modules excluded): `bash` 1167 and `sh` 38 are the
 * ONLY shell identifiers that occur at all — `shell`, `zsh`, `console`,
 * `shell-session` and `terminal` appear zero times. The rest of this set is
 * there for model output the repository's own prose does not exercise.
 *
 * Windows shells (`powershell`, `bat`, `cmd`) are deliberately absent. The dock
 * does spawn `ComSpec`/cmd.exe on Windows (main.ts), so they are a defensible
 * future addition — but a `powershell` fence handed to a POSIX login shell on
 * the machine this was built and tested on is a command that cannot work, and
 * adding an identifier that is right on one platform and wrong on the other
 * needs the platform test this module does not have.
 */
const RUNNABLE_SHELL_LANGUAGES: ReadonlySet<string> = new Set([
  'bash',
  'sh',
  'zsh',
  'ksh',
  'fish',
  'shell',
  'shellscript',
  'bashscript',
]);

/**
 * The longest command Run will offer.
 *
 * Not a security rail — the dock is already an unrestricted shell (see below) —
 * but a shape rail. Past a few hundred lines a fence is a FILE the reader is
 * meant to save, not a command they meant to type, and pasting one into a line
 * editor jams it rather than running anything useful. Copy still covers that
 * case, which is why hiding the button costs the user nothing.
 */
export const MAX_RUNNABLE_COMMAND_CHARS = 8000;

/**
 * Is this fence identifier one whose body is a runnable shell command?
 *
 * Case-insensitive, since a fence is written by hand and ```Bash happens.
 */
export function isRunnableShellLanguage(language: string | null | undefined): boolean {
  if (!language) return false;
  return RUNNABLE_SHELL_LANGUAGES.has(language.toLowerCase());
}

/**
 * C0 controls minus tab, newline and carriage return, plus DEL.
 *
 * Written with codepoint escapes on purpose: a literal control byte in a
 * source file is invisible in every diff that would review it.
 */
// Matching control characters IS the point here; no-control-regex exists to
// catch one that arrived by accident.
// eslint-disable-next-line no-control-regex
const CONTROL_CHARS = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g;

/**
 * The command a code block would run, or null when there is nothing to run.
 *
 * Control characters are STRIPPED rather than passed through, and that is the
 * one genuinely new risk this feature carries. Execution needs no new gate —
 * the dock is already an unrestricted user shell with no confirmation and no
 * allowlist — but until now every byte in it was typed by the user. This text
 * is MODEL OUTPUT, and a bare ESC in it is not a command: it either terminates
 * the bracketed paste early (`\x1b[201~`, after which the remainder is
 * executed as keystrokes rather than inserted as text) or acts as a control
 * key the user never pressed. A literal ESC byte inside a markdown fence is
 * noise or an attack in every realistic case; an escape someone genuinely
 * wants to emit is written `\033`, which is four ordinary characters and
 * survives untouched.
 *
 * Tab and the newlines are kept: they are layout, and the newlines are what the
 * caller normalizes into carriage returns.
 */
export function runnableCommandFromBlock(source: string): string | null {
  // C0 minus \t \n \r, plus DEL.
  const stripped = source.replace(CONTROL_CHARS, '');
  const trimmed = stripped.trim();
  if (!trimmed) return null;
  if (trimmed.length > MAX_RUNNABLE_COMMAND_CHARS) return null;
  return trimmed;
}

/** Both halves of the decision, for a caller that holds a fence and its body. */
export function runnableCommandFromCodeBlock(
  language: string | null | undefined,
  source: string
): string | null {
  if (!isRunnableShellLanguage(language)) return null;
  return runnableCommandFromBlock(source);
}

/** Bracketed paste, per https://cirw.in/blog/bracketed-paste. */
const PASTE_START = '\x1b[200~';
const PASTE_END = '\x1b[201~';

/** Enter, as xterm sends it — never `\n`. */
export const SUBMIT = '\r';

/**
 * The exact bytes a Run click writes into the pty.
 *
 * Newlines become carriage returns, exactly as `prepareTextForTerminal` in
 * xterm's own Clipboard.ts does for a real paste — a pty's line editor accepts
 * CR, and a raw `\n` is a different key.
 *
 * `bracketedPaste` must be read from the terminal's own
 * `modes.bracketedPasteMode`, never assumed. It reports whether the SHELL asked
 * for bracketed paste (DECSET 2004), and the two answers are not
 * interchangeable: with it on, a multi-line block lands in the line editor as
 * one literal buffer that the trailing CR then submits, so a heredoc or a
 * `for` loop runs as written; with it off — a plain `sh`, a shell that turned
 * it off — those same six bytes are not a mode, they are the literal text
 * `[200~` typed in front of the user's command.
 *
 * The trailing CR sits OUTSIDE the bracket, because that is the user's Enter
 * and not part of the pasted text. Sending it at all is the deliberate part of
 * this feature: the click is the consent, so the command runs.
 */
export function terminalInputForCommand(command: string, bracketedPaste: boolean): string {
  const text = command.replace(/\r?\n/g, '\r');
  const pasted = bracketedPaste ? `${PASTE_START}${text}${PASTE_END}` : text;
  return `${pasted}${SUBMIT}`;
}
