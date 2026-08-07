import { useCallback, useEffect, useState } from 'react';
import { Switch } from '../../ui/switch';
import { Button } from '../../ui/button';
import { Input } from '../../ui/input';
import { useConfig } from '../../ConfigContext';
import { useDisclosure } from '../../privacy/disclosureCopy';
import { DisclosureProse } from '../../privacy/DisclosureProse';
import { DISABLE_PHRASE, PRIVACY_TIERS_KEY, privacyTiersEnabledFromConfig } from './privacyTiers';

// Re-exported so the panel stays the name every existing importer already
// reaches for; the definitions live in `privacyTiers.ts` because
// `ConfigContext` and `PrivacyBadge` read them too and must not import a
// settings screen.
export { DISABLE_PHRASE, PRIVACY_TIERS_KEY, privacyTiersEnabledFromConfig };

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
  // Task 30A (DR-17 req. 3). ⚠ Unconditional — the copy is fetched and shown in
  // BOTH toggle positions. Passing `enabled` here would be the plausible wrong
  // implementation: it would take the disclosure away in exactly the
  // configuration where the exposure is largest.
  const { copy: disclosure } = useDisclosure();
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
          which conversations touch private material. Every badge in the app reads{' '}
          <em>enforcement off</em> while this is the case.
        </div>
      )}

      {/*
        Issue #56, DR-17 requirement 3 — the permanent statement of what a
        non-private model can reach.

        ⚠ Three things about this block are load-bearing.

        1. It is ABOVE the switch. A user who reads only the first thing on the
           screen has to meet the limit before the control.
        2. It renders in BOTH toggle positions. DR-15 turns off gates, the
           ratchet and refusals; it does not turn off the truth, and with
           enforcement off the exposure is larger, not smaller.
        3. Every word comes from the daemon. There is no fallback string — if
           the copy cannot be fetched this renders nothing, because inventing
           prose here is the drift the one-definition rule exists to prevent.
      */}
      {disclosure && (
        <div
          data-testid="non-private-model-statement"
          className="rounded-lg border border-borderStandard px-3 py-3 space-y-2 text-sm text-text-default"
        >
          <DisclosureProse
            text={disclosure.long}
            paragraphClassName="min-w-0 [overflow-wrap:anywhere]"
          />
          {!enabled && (
            // The served copy names three things Biorouter "does stop". With
            // the master switch off it stops none of them, so reprinting that
            // paragraph unqualified would be a false statement on the very
            // screen the user turned it off from.
            <p className="min-w-0 text-text-muted">
              <strong>
                Those three are not being stopped right now — privacy tiers are off on this machine.
              </strong>{' '}
              Turning them back on restores them for what is already marked private.
            </p>
          )}
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
