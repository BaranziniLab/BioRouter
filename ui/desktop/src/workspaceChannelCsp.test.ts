import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

/**
 * BR-71 §4.3: the renderer opens ONE WebSocket per window to
 * `GET /ui/workspace` (`hooks/useWorkspaceChannel.ts`). It is the only
 * WebSocket the main renderer document opens — every other daemon call is
 * `fetch`/SSE over `http://127.0.0.1:<port>` — so it is the first request in
 * the app to need a `ws:` source in `connect-src`.
 *
 * CSP does NOT let an `http:` source expression cover a `ws:` URL. CSP3
 * §6.6.2.6 allows exactly three scheme relaxations — `http`→`https`,
 * `ws`→`wss`, and `ws`→`http`/`https` — and `http`→`ws` is not among them, so
 * `connect-src http://127.0.0.1:*` blocks `ws://127.0.0.1:62736/ui/workspace`
 * outright.
 *
 * That is not a theoretical reading. Measured in the dev GUI during Task 31's
 * live pass: the socket never connected, `securitypolicyviolation` fired with
 * `effectiveDirective: "connect-src"` and
 * `blockedURI: "ws://127.0.0.1:62736/ui/workspace?…"`, and every
 * `workspace_list` therefore reported `"gui_attached": false` while the GUI was
 * running in front of the user — which silently disables the whole GUI half of
 * the feature (no tabs open, `workspace_open` degrades, Decision 4's approval
 * guard mis-fires).
 *
 * Both policies have to allow it, because both apply to the window:
 *   - the `<meta http-equiv="Content-Security-Policy">` in `index.html`
 *   - the header `main.ts` attaches in `session.defaultSession.webRequest
 *     .onHeadersReceived` (built by `buildConnectSrc()`)
 * The most restrictive of the two wins, so allowing it in one and not the other
 * still blocks the socket. These assertions read the real shipped sources
 * rather than a copy.
 */

function resolveFromPackage(relative: string): string {
  const candidates = [join(process.cwd(), relative), join(process.cwd(), 'ui', 'desktop', relative)];
  const found = candidates.find((p) => existsSync(p));
  if (!found) throw new Error(`could not locate ${relative} from ${process.cwd()}`);
  return found;
}

/** The `connect-src …;` run out of a full CSP string. */
function connectSrcOf(policy: string): string {
  const match = policy.match(/connect-src([^;]*)/);
  if (!match) throw new Error(`no connect-src directive in policy: ${policy.slice(0, 200)}`);
  return match[1].trim();
}

function metaCsp(): string {
  const html = readFileSync(resolveFromPackage('index.html'), 'utf-8');
  const match = html.match(
    /<meta\s+http-equiv="Content-Security-Policy"\s+content="([^"]*)"\s*\/?>/i
  );
  if (!match) throw new Error('index.html has no Content-Security-Policy meta tag');
  return match[1];
}

/** The literal source list `main.ts`'s `buildConnectSrc()` starts from. */
function mainProcessConnectSources(): string {
  const source = readFileSync(resolveFromPackage('src/main.ts'), 'utf-8');
  const match = source.match(/const buildConnectSrc = \(\): string => \{\s*const sources = \[([^\]]*)\]/);
  if (!match) throw new Error('main.ts: could not find buildConnectSrc’s source list');
  return match[1];
}

describe('workspace channel CSP', () => {
  it('the index.html meta policy allows a ws: connection to the loopback daemon', () => {
    const connectSrc = connectSrcOf(metaCsp());
    expect(connectSrc).toContain('ws://127.0.0.1:*');
  });

  it('the main-process header policy allows a ws: connection to the loopback daemon', () => {
    expect(mainProcessConnectSources()).toContain('ws://127.0.0.1:*');
  });

  it('does not settle for the http: source, which CSP will not stretch to ws:', () => {
    // Guards the exact mistake this test exists for: someone "fixing" the
    // socket by widening the http entry (or dropping to `connect-src *`)
    // instead of naming the ws scheme.
    for (const connectSrc of [connectSrcOf(metaCsp()), mainProcessConnectSources()]) {
      expect(connectSrc).toContain('127.0.0.1');
      expect(connectSrc).not.toContain("'unsafe-eval'");
      expect(connectSrc.replace(/ws:\/\/127\.0\.0\.1:\*/g, '')).not.toMatch(/(^|\s)\*(\s|$)/);
    }
  });
});
