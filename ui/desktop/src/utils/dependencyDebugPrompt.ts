/**
 * dependencyDebugPrompt.ts
 *
 * Turns a failed dependency/prerequisite install into a self-contained briefing
 * for a fresh Biorouter chat.
 *
 * The point of the feature: when an install fails, the user is holding an error
 * string and a dead end. Biorouter already has an agent with shell access that
 * is good at exactly this. So the failure carries an action that opens a NEW
 * chat — never the one the user was working in — seeded with everything the
 * agent needs to diagnose it without asking twenty questions first.
 *
 * This module is deliberately pure: no Electron, no React, no IPC. The
 * composition rules (what context is included, what the agent is asked to do,
 * how output is truncated) are the part worth testing, and a pure function is
 * the only way to test them without standing up a window.
 */

/** Where the failure came from. Shapes the framing, not the mechanics. */
export type DependencyFailureKind =
  /** A system prerequisite: git, python, uv, npm, rust, aws, llama-server. */
  | 'dependency'
  /** Putting the bundled `biorouter` CLI onto PATH. */
  | 'cli'
  /** A `.brxt` extension bundle install or update. */
  | 'extension'
  /** A prerequisite check from a setup/build script run outside the app. */
  | 'script';

export interface DependencyFailure {
  kind: DependencyFailureKind;
  /** Machine name, e.g. `uv`. */
  name: string;
  /** Human name, e.g. `uv (Python package manager)`. Falls back to `name`. */
  displayName?: string;
  /** The command that was run, verbatim. */
  command?: string;
  /** Process exit code, when the command ran at all. */
  exitCode?: number | null;
  /** Combined stdout/stderr captured while it ran. */
  output?: string;
  /** The error message shown to the user. */
  error?: string;
  /** Official download/docs page, when there is one. */
  downloadUrl?: string;
  /** Whether the install command needs elevated privileges. */
  requiresSudo?: boolean;
  /** Environment as the app sees it — routinely different from the login shell. */
  environment?: DependencyEnvironment;
}

export interface DependencyEnvironment {
  platform?: string;
  arch?: string;
  appVersion?: string;
  osRelease?: string;
  /** PATH the app searches, after Biorouter's own augmentation. */
  augmentedPath?: string;
  /** PATH the app inherited from launchd/the desktop session. */
  inheritedPath?: string;
  homedir?: string;
}

/**
 * Captured output can be megabytes (a `uv sync` compiling from source). The tail
 * is where the failure is, so keep the tail.
 */
export const MAX_OUTPUT_CHARS = 4_000;

/** Rough upper bound on the "…N earlier characters omitted…" marker. */
const OMISSION_MARKER_CHARS = 64;

export function truncateOutput(output: string, max = MAX_OUTPUT_CHARS): string {
  const trimmed = output.trimEnd();
  // Truncating something only slightly over the cap would ADD the marker and
  // come out longer than what it replaced. Only cut when cutting actually saves.
  if (trimmed.length <= max + OMISSION_MARKER_CHARS) return trimmed;
  const kept = trimmed.slice(-max);
  const dropped = trimmed.length - max;
  return `…[${dropped.toLocaleString()} earlier characters omitted]…\n${kept}`;
}

function fence(body: string, lang = ''): string {
  // A fence inside the body would end ours early; lengthen the delimiter past
  // the longest run already present rather than mangling the output.
  const longest = (body.match(/`{3,}/g) ?? []).reduce((n, m) => Math.max(n, m.length), 0);
  const delim = '`'.repeat(Math.max(3, longest + 1));
  return `${delim}${lang}\n${body}\n${delim}`;
}

const KIND_SUBJECT: Record<DependencyFailureKind, string> = {
  dependency: 'a required system dependency',
  cli: 'the Biorouter command-line tool',
  extension: 'a Biorouter extension',
  script: 'a prerequisite for a Biorouter setup script',
};

/** Short label for the button/menu that launches the session. */
export function debugSessionTitle(failure: DependencyFailure): string {
  return `Fix ${failure.displayName || failure.name}`;
}

/**
 * Build the message the new chat opens with.
 *
 * Structure is fixed on purpose: goal, what failed, the evidence, the
 * environment, then the rules. The evidence sits above the rules so a model that
 * skims still reads the error.
 */
export function buildDependencyDebugPrompt(failure: DependencyFailure): string {
  const label = failure.displayName || failure.name;
  const env = failure.environment ?? {};
  const parts: string[] = [];

  parts.push(
    `I am trying to install **${label}**, ${KIND_SUBJECT[failure.kind]}, and it failed. ` +
      `Please diagnose why and fix it on this machine.`
  );

  const facts: string[] = [];
  facts.push(`- What failed: ${label} (\`${failure.name}\`)`);
  if (failure.command) facts.push(`- Command Biorouter ran: \`${failure.command}\``);
  if (failure.exitCode !== undefined && failure.exitCode !== null) {
    facts.push(`- Exit code: ${failure.exitCode}`);
  }
  if (failure.error) facts.push(`- Error reported: ${failure.error}`);
  if (failure.requiresSudo) {
    facts.push('- This install normally needs administrator privileges.');
  }
  if (failure.downloadUrl) facts.push(`- Official install page: ${failure.downloadUrl}`);
  parts.push(`## What failed\n\n${facts.join('\n')}`);

  if (failure.output && failure.output.trim()) {
    parts.push(`## Output from the failed command\n\n${fence(truncateOutput(failure.output))}`);
  }

  const envLines: string[] = [];
  if (env.platform) envLines.push(`- Platform: ${env.platform}${env.arch ? ` (${env.arch})` : ''}`);
  if (env.osRelease) envLines.push(`- OS: ${env.osRelease}`);
  if (env.appVersion) envLines.push(`- Biorouter version: ${env.appVersion}`);
  if (env.homedir) envLines.push(`- Home directory: ${env.homedir}`);
  if (env.augmentedPath) {
    envLines.push(
      `- PATH Biorouter searches:\n${fence(env.augmentedPath.split(/[:;]/).filter(Boolean).join('\n'))}`
    );
  }
  if (env.inheritedPath && env.inheritedPath !== env.augmentedPath) {
    envLines.push(
      '- Note: the desktop app is launched by the OS, not by a login shell, so it does not ' +
        'see the PATH from `.zshrc`/`.bashrc`. A tool can be installed and still be invisible ' +
        'to the app. That is worth checking before assuming it is missing.'
    );
  }
  if (envLines.length) parts.push(`## This machine\n\n${envLines.join('\n')}`);

  parts.push(
    `## What I need from you

1. Work out the actual cause from the output above — not the most common cause.
2. Fix it using the shell. Prefer the least invasive fix that works.
3. Verify by re-running the check yourself (\`${verifyCommand(failure)}\`) and show me the result.
4. Tell me in plain language what was wrong and what you changed.

Rules:
- Ask me first before anything needing \`sudo\`, anything that removes or downgrades software I did not ask about, or anything that edits my shell profile.
- If the fix is not possible without me doing something by hand, say so and give me the exact steps.
- If the tool turns out to be installed already and merely not visible to Biorouter, say that — the fix is the PATH, not another install.`
  );

  return parts.join('\n\n');
}

/** The command that proves the fix worked. */
export function verifyCommand(failure: DependencyFailure): string {
  switch (failure.kind) {
    case 'cli':
      return 'biorouter --version';
    case 'extension':
      return 'uv sync';
    case 'dependency':
    case 'script':
    default:
      return `${failure.name} --version`;
  }
}
