import { describe, expect, it } from 'vitest';
import {
  MAX_RUNNABLE_COMMAND_CHARS,
  isRunnableShellLanguage,
  runnableCommandFromBlock,
  runnableCommandFromCodeBlock,
  terminalInputForCommand,
} from './shellCommandBlock';

const ESC = String.fromCharCode(0x1b);

describe('isRunnableShellLanguage', () => {
  it('accepts the shell fences that actually occur', () => {
    // bash (1167) and sh (38) are the only two present in this repo's corpus.
    for (const language of ['bash', 'sh', 'zsh', 'ksh', 'fish', 'shell', 'shellscript']) {
      expect(isRunnableShellLanguage(language)).toBe(true);
    }
  });

  it('is case-insensitive, because a fence is written by hand', () => {
    expect(isRunnableShellLanguage('Bash')).toBe(true);
    expect(isRunnableShellLanguage('SH')).toBe(true);
  });

  it('refuses non-shell fences', () => {
    for (const language of ['rust', 'ts', 'tsx', 'python', 'json', 'yaml', 'sql', 'js']) {
      expect(isRunnableShellLanguage(language)).toBe(false);
    }
  });

  it('refuses prose and patch fences that are not commands at all', () => {
    for (const language of ['text', 'markdown', 'md', 'diff', 'plaintext']) {
      expect(isRunnableShellLanguage(language)).toBe(false);
    }
  });

  it('refuses TRANSCRIPT fences, whose bodies are a prompt plus output', () => {
    // Running `$ ls` verbatim executes the prompt character, and the lines
    // after it are output, not commands.
    for (const language of ['console', 'shell-session', 'shellsession', 'sh-session', 'terminal']) {
      expect(isRunnableShellLanguage(language)).toBe(false);
    }
  });

  it('refuses Windows shells, which the POSIX login shell cannot run', () => {
    for (const language of ['powershell', 'ps1', 'bat', 'cmd', 'batch']) {
      expect(isRunnableShellLanguage(language)).toBe(false);
    }
  });

  it('refuses an absent language, which is what an untagged fence has', () => {
    expect(isRunnableShellLanguage(null)).toBe(false);
    expect(isRunnableShellLanguage(undefined)).toBe(false);
    expect(isRunnableShellLanguage('')).toBe(false);
  });

  it('rejects `shell-session` on the FULL token, not the truncated one', () => {
    // The trap this predicate exists to close: MarkdownCode's own
    // `/language-(\w+)/` stops at the hyphen, so `language-shell-session`
    // yields the string `shell`, which IS runnable. Feed the predicate the
    // truncation and it says yes — which is why the caller must pass the whole
    // identifier.
    expect(isRunnableShellLanguage('shell')).toBe(true);
    expect(isRunnableShellLanguage('shell-session')).toBe(false);
  });
});

describe('runnableCommandFromBlock', () => {
  it('returns the command, trimmed at the ends', () => {
    expect(runnableCommandFromBlock('\n  ls -la  \n\n')).toBe('ls -la');
  });

  it('keeps interior indentation, which is part of a script', () => {
    expect(runnableCommandFromBlock('for f in *; do\n  echo "$f"\ndone')).toBe(
      'for f in *; do\n  echo "$f"\ndone'
    );
  });

  it('returns null for an empty or whitespace-only block', () => {
    expect(runnableCommandFromBlock('')).toBeNull();
    expect(runnableCommandFromBlock('   \n\t\n  ')).toBeNull();
  });

  it('returns null past the length rail', () => {
    expect(runnableCommandFromBlock('x'.repeat(MAX_RUNNABLE_COMMAND_CHARS))).not.toBeNull();
    expect(runnableCommandFromBlock('x'.repeat(MAX_RUNNABLE_COMMAND_CHARS + 1))).toBeNull();
  });

  it('strips a bare ESC, which could otherwise close the paste bracket early', () => {
    // Everything after a literal `\x1b[201~` would be executed as keystrokes
    // rather than inserted as text.
    const attack = `echo safe${ESC}[201~; rm -rf /tmp/x`;
    const command = runnableCommandFromBlock(attack);
    expect(command).not.toBeNull();
    expect(command).not.toContain(ESC);
    expect(command).toBe('echo safe[201~; rm -rf /tmp/x');
  });

  it('strips other C0 controls and DEL', () => {
    const noisy = `ls${String.fromCharCode(0x03)}${String.fromCharCode(0x00)}${String.fromCharCode(0x7f)} -la`;
    expect(runnableCommandFromBlock(noisy)).toBe('ls -la');
  });

  it('keeps tabs and newlines, which are layout rather than control', () => {
    expect(runnableCommandFromBlock('echo\ta\necho b')).toBe('echo\ta\necho b');
  });

  it('leaves an escape written as source text alone', () => {
    // `\033` is four ordinary characters; only a literal ESC byte is stripped.
    expect(runnableCommandFromBlock("printf '\\033[31mred\\033[0m'")).toBe(
      "printf '\\033[31mred\\033[0m'"
    );
  });
});

describe('runnableCommandFromCodeBlock', () => {
  it('requires both a shell language and a non-empty body', () => {
    expect(runnableCommandFromCodeBlock('bash', 'ls')).toBe('ls');
    expect(runnableCommandFromCodeBlock('rust', 'ls')).toBeNull();
    expect(runnableCommandFromCodeBlock('bash', '   ')).toBeNull();
  });
});

describe('terminalInputForCommand', () => {
  it('submits: the click is the consent, so the command runs', () => {
    expect(terminalInputForCommand('ls -la', false)).toBe('ls -la\r');
  });

  it('brackets the paste when the shell asked for it', () => {
    expect(terminalInputForCommand('ls -la', true)).toBe(`${ESC}[200~ls -la${ESC}[201~\r`);
  });

  it('puts the submitting CR OUTSIDE the bracket — it is Enter, not text', () => {
    const bytes = terminalInputForCommand('ls', true);
    expect(bytes.endsWith(`${ESC}[201~\r`)).toBe(true);
  });

  it('sends a multi-line block as ONE bracketed buffer with one Enter', () => {
    // The realistic failure: line-by-line, a heredoc's lines each arrive as a
    // separate Enter. Bracketed, the whole thing lands in the line editor and
    // the single trailing CR runs it.
    const heredoc = 'cat <<EOF\nhello\nEOF';
    expect(terminalInputForCommand(heredoc, true)).toBe(
      `${ESC}[200~cat <<EOF\rhello\rEOF${ESC}[201~\r`
    );
  });

  it('normalizes newlines to CR — a pty line editor accepts CR, not LF', () => {
    expect(terminalInputForCommand('a\nb', false)).toBe('a\rb\r');
    expect(terminalInputForCommand('a\r\nb', false)).toBe('a\rb\r');
  });

  it('never emits a bare LF', () => {
    expect(terminalInputForCommand('a\nb\r\nc', true)).not.toContain('\n');
  });
});
