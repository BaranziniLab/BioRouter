import { useCallback, useEffect, useState } from 'react';
import { Switch } from '../../ui/switch';
import { Button } from '../../ui/button';
import { Input } from '../../ui/input';
import { useConfig } from '../../ConfigContext';
import { disclosureTitle, useDisclosure } from '../../privacy/disclosureCopy';
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
        // P-05. NEVER `setEnabled(on)` — that is the value the user ASKED for,
        // and this switch may only ever show the value the daemon HOLDS.
        //
        // A resolved write is not proof the write landed: the master switch is
        // the one key `/config/upsert` can refuse after accepting the request
        // (DR-20's operating-system authentication is raised inside the
        // handler), and any future refusal that the client failed to raise as
        // an exception would be painted here as a successful disable. So the
        // panel asks rather than assumes, and a disagreement is reported as a
        // refusal instead of being displayed as a success.
        const applied = privacyTiersEnabledFromConfig(await read(PRIVACY_TIERS_KEY, false));
        setEnabled(applied);
        if (applied !== on) {
          setError(
            'Biorouter did not apply that change. Privacy tiers are still ' +
              (applied ? 'on' : 'off') +
              '. Nothing was changed.'
          );
          return;
        }
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
    [upsert, read, refresh]
  );

  if (enabled === null) {
    return <div className="text-sm text-text-muted">Loading privacy settings…</div>;
  }

  return (
    // `data-privacy-panel` marks this section's root so SettingsView's suite
    // can assert WHERE it sits among its siblings, not merely that it mounted.
    <section className="space-y-4" data-privacy-panel>
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
          {/* The subject the prose refers to.
              `disclosure.long` opens "It is not HIPAA-compliant, …" and the
              modal binds that "It" with the served heading above it. This pane
              reused the paragraphs WITHOUT the heading, so the first words of
              Settings → Privacy were a pronoun with no antecedent anywhere on
              the page.

              The heading is the served template, not a sentence written here —
              the one-definition rule holds. Only the substitution differs: the
              modal names the provider it is about to open, and this pane is
              about the whole class, so the class is what fills the slot. */}
          <p className="text-label text-text-default">
            {disclosureTitle(disclosure, 'A non-private model')}
          </p>
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
          {/* ⚠ Two clauses, not three, and the third is the reason. It used to
              end "and can't reach your knowledge bases through the shell",
              which is false: the general filesystem read-deny did not ship, and
              the served disclosure four elements above this one says so. A
              panel contradicting the disclosure it renders is worse than either
              statement alone. The triad also read as rhythm rather than fact —
              the limb that was doing the least work was the one that was wrong.
              Private is defined by the rule, not by naming a provider. */}
          <p className="text-xs text-text-muted mt-0.5">
            Chats on private models stay private: a public model can&rsquo;t read them and
            can&rsquo;t call a private extension. A private model is one your institution hosts,
            or one that runs on this machine.
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

      {/*
        P-05. The refusal lives HERE, at section level, not inside the
        confirmation dialog it used to be nested in.

        Both directions of this switch can be refused by the daemon, but only
        the OFF direction opens the dialog. So while the message was nested, a
        failed *enable* — the direction that restores protection — printed
        nothing at all: the switch flicked back to off and the user was left to
        infer that their click had been dropped. A control whose failure is
        silent in one direction is the same defect as one whose failure is
        silent in both.
      */}
      {error && (
        <p
          role="alert"
          data-testid="privacy-toggle-error"
          className="min-w-0 rounded-lg border border-borderStandard px-3 py-2 text-xs text-text-default [overflow-wrap:anywhere]"
        >
          {error}
        </p>
      )}

      {confirming && (
        <div
          data-testid="privacy-disable-confirm"
          className="rounded-lg border border-borderStandard px-3 py-3 space-y-3"
        >
          <p className="text-sm text-text-default">
            This turns off <strong>every</strong> privacy guardrail on this machine, for every
            conversation.
          </p>
          {/* ⚠ The shell clause moved OUT of this list, and that is the point.
              Everything here is a consequence of turning the switch off, so
              listing shell reads among them told the user they were prevented
              while it was on. They are not: §9.5's filesystem read-deny did not
              ship. Naming it separately keeps the warning honest in both
              directions — the list stays true, and the thing that is never
              blocked is still disclosed rather than quietly dropped. */}
          <p className="text-sm text-text-default">
            Commercial models will be able to call your private extensions, read private chat
            history, and read and write your knowledge bases.
          </p>
          <p className="text-sm text-text-default">
            Reading your saved chats, memories and Biorouter apps off the disk through the shell is
            not blocked either way.
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
