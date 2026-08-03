import { useCallback, useEffect, useState } from 'react';
import { Switch } from '../../ui/switch';
import { Button } from '../../ui/button';
import { Input } from '../../ui/input';
import { useConfig } from '../../ConfigContext';

/**
 * The config key that holds the master switch. One spelling, shared with the
 * daemon's `biorouter::privacy::PRIVACY_TIERS_CONFIG_KEY`.
 */
export const PRIVACY_TIERS_KEY = 'BIOROUTER_PRIVACY_TIERS';

/**
 * The phrase the user must type to turn the feature off, byte-for-byte the
 * daemon's `PRIVACY_TIERS_DISABLE_PHRASE`. The daemon compares it EXACTLY, so a
 * panel that lower-cased or trimmed it would produce a 403 the user cannot
 * explain.
 */
export const DISABLE_PHRASE = 'DISABLE PRIVACY TIERS';

/**
 * `off` is the only value that disables; anything else, including an absent key,
 * is on. Mirrors `biorouter::privacy::privacy_tiers_value_is_on` — the daemon is
 * the authority and this is only what the panel renders before the next read.
 */
export function privacyTiersEnabledFromConfig(value: unknown): boolean {
  if (typeof value === 'boolean') return value;
  if (typeof value !== 'string') return true;
  const v = value.trim().toLowerCase();
  return !(v === 'off' || v === 'false' || v === 'no');
}

/**
 * Settings → Privacy (issue #56, DR-15).
 *
 * ONE switch, on by default, governing the whole privacy-tier feature. Turning
 * it **off** requires typing {@link DISABLE_PHRASE} and shows, verbatim, all
 * four sentences below — the third and fourth are the ones a user cannot
 * reconstruct for themselves and are why this is a typed confirmation rather
 * than a switch.
 *
 * ⚠ The knowledge-base barrier is NOT carved out of this. An earlier draft kept
 * it enforced regardless, on the grounds that a KB carries session contents and
 * has no declassification path (AR-1). That reasoning survives as a *cost* and
 * is why the copy names knowledge bases explicitly — but it is no longer an
 * exception. A user who turns the feature off gets a machine on which a public
 * model can read a private base, and the dialog says so in those words.
 */
export default function PrivacyPanel() {
  const { read, upsert } = useConfig();
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [typed, setTyped] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setEnabled(privacyTiersEnabledFromConfig(await read(PRIVACY_TIERS_KEY, false)));
    } catch {
      // An unreadable key resolves to ON, exactly as the daemon's loader does:
      // the failure of a read must not be a way to *display* the feature as off.
      setEnabled(true);
    }
  }, [read]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const write = useCallback(
    async (on: boolean) => {
      setBusy(true);
      setError(null);
      try {
        // The confirmation rides on every write of this key, in both directions.
        // The daemon refuses a BARE upsert of it whichever value is being
        // written, so turning protection back ON needs the phrase too — which
        // costs the user nothing, because the panel supplies it, and keeps the
        // daemon's rule to a single unconditional branch.
        await upsert(PRIVACY_TIERS_KEY, on ? 'on' : 'off', false, DISABLE_PHRASE);
        setEnabled(on);
        setConfirming(false);
        setTyped('');
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        // Re-read rather than assume: a refused write leaves the daemon's value
        // where it was, and the switch must not be left showing what the user
        // asked for instead of what is true.
        await refresh();
      } finally {
        setBusy(false);
      }
    },
    [upsert, refresh]
  );

  if (enabled === null) {
    return <div className="text-sm text-text-muted">Loading privacy settings…</div>;
  }

  return (
    <section className="space-y-4">
      {!enabled && (
        <div
          data-testid="privacy-enforcement-off-strip"
          role="status"
          className="rounded-lg border border-borderStandard bg-background-muted px-3 py-2 text-sm text-text-default"
        >
          <strong>Privacy tiers are off.</strong> Nothing on this machine is separating private
          chats, extensions or knowledge bases from public models, and Biorouter is not recording
          which conversations touch private material.
        </div>
      )}

      <div className="biorouter-settings-row flex items-start justify-between gap-4 px-3 py-2.5">
        <div className="min-w-0">
          <p className="text-sm font-medium text-text-default">Privacy tiers</p>
          <p className="text-xs text-text-muted mt-0.5">
            Chats on private models (Versa, or a local model) stay private: a public model
            can&rsquo;t read them, can&rsquo;t call a private extension, and can&rsquo;t reach your
            knowledge bases through the shell.
          </p>
        </div>
        <Switch
          checked={enabled}
          disabled={busy}
          variant="mono"
          aria-label="Privacy tiers"
          onCheckedChange={(next) => {
            if (next) {
              void write(true);
            } else {
              // Off is the direction that needs the typed confirmation; on is a
              // plain flip, because raising protection needs no ceremony.
              setConfirming(true);
              setTyped('');
              setError(null);
            }
          }}
        />
      </div>

      {confirming && (
        <div
          data-testid="privacy-disable-confirm"
          className="rounded-lg border border-borderStandard px-3 py-3 space-y-3"
        >
          <p className="text-sm text-text-default">
            This turns off <strong>every</strong> privacy guardrail on this machine, for every
            conversation.
          </p>
          <p className="text-sm text-text-default">
            Commercial models will be able to call UCSF clinical extensions, read private chat
            history, read and write your knowledge bases, and read your saved chats, memories and
            Biorouter apps straight off the disk through the shell.
          </p>
          <p className="text-sm text-text-default">
            <strong>
              While it is off, Biorouter stops recording which conversations touched private
              material.
            </strong>
          </p>
          <p className="text-sm text-text-default">
            Turning it back on will protect what is already marked private — but it cannot go back
            and mark anything that happened while it was off.
          </p>
          <label className="block text-xs text-text-muted" htmlFor="privacy-disable-phrase">
            Type <code>{DISABLE_PHRASE}</code> to continue.
          </label>
          <Input
            id="privacy-disable-phrase"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={DISABLE_PHRASE}
            aria-label="Confirmation phrase"
          />
          {error && (
            <p role="alert" className="text-xs text-text-muted">
              {error}
            </p>
          )}
          <div className="flex gap-2">
            <Button
              variant="destructive"
              size="sm"
              // EXACT comparison, matching the daemon's. A case-insensitive or
              // trimmed match here would let the panel send a phrase the daemon
              // then refuses, which surfaces as an unexplained 403.
              disabled={typed !== DISABLE_PHRASE || busy}
              onClick={() => void write(false)}
            >
              Turn off privacy tiers
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setConfirming(false);
                setTyped('');
                setError(null);
              }}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}
