import { useEffect, useMemo, useState } from 'react';
import { AlertTriangle } from '../../icons/app-icons';
import type { KbFormat } from '../../../api/types.gen';
import { Badge } from '../../ui/badge';
import { Button } from '../../ui/button';
import { Input } from '../../ui/input';
import CustomRadio from '../../ui/CustomRadio';
import { ModalShell } from '../../ModalShell';
import { previewKbId } from '../kbFormat';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Prefill the name field — the manager's search box, when the user typed one. */
  initialName?: string;
  /** Create it. Resolves with the id the DAEMON assigned, which may differ. */
  onCreate: (name: string, format: KbFormat) => Promise<string | undefined>;
}

/**
 * One row of the format radiogroup: the name, its short code, the "pick this
 * when" line, and the three facts under it.
 *
 * The three slots map onto `CustomRadio`'s own — `label`, `secondaryLabel`,
 * `rightContent` — rather than re-rolling a `.biorouter-list-row` around a
 * radio glyph. The primitive already renders its own `<label htmlFor>`, so
 * nesting it inside a `div role="radio"` would put a `<label>` inside a radio
 * role and leave three slots unused, and it would author a fourth radio
 * geometry beside the 22px-ring-in-a-24px-target one that ships.
 *
 * ⚠ **The badge carries the SHORT CODE and the label carries what it is**, which
 * is a deliberate reading of §4.3's "the format name plus its `Badge uppercase`
 * short code". Taken literally the two are the same word, so the row rendered as
 * `OKF ⟨OKF⟩` — a stutter that teaches nothing. Splitting them gives the badge a
 * job (it is the same mark the subject band, the picker and the manager row
 * show, so the user learns the mapping at the moment of choosing) and the label
 * a job (what the format is for).
 */
const FORMATS: {
  value: KbFormat;
  name: string;
  code: string;
  when: string;
  facts: [string, string, string];
}[] = [
  {
    value: 'okf',
    name: 'General knowledge',
    code: 'OKF',
    when: 'You are keeping notes, project context, retrieval material, or anything that is not curated biology.',
    facts: [
      'Any page type, any link name. Nothing is ever rejected.',
      'Best when you do not yet know how the material will be structured.',
      'Validation reports broken links only.',
    ],
  },
  {
    value: 'biookf',
    name: 'Curated biomedical',
    code: 'BioOKF',
    when: 'You are curating biomedical literature or building a base another institution will read.',
    facts: [
      '28 page types and 35 link predicates, checked.',
      'Every link must name its evidence: knowledge level, agent type, and a primary source.',
      'Validation flags anything outside the vocabulary and names the closest legal value.',
    ],
  },
];

/**
 * The format chooser (ui-spec §4.3) — the surface that makes OKF versus BioOKF
 * a decision the user makes rather than one the daemon defaults.
 *
 * ⚠ **The id line is a PREVIEW and the copy says so.** Slug derivation and
 * collision handling are owned by `create_base_as` on the daemon, not by the
 * renderer, so a client-derived id the server then alters is a real footgun. The
 * created base's actual id is echoed back through `onCreate`'s resolution.
 *
 * ⚠ **The irreversibility banner is not decoration.** `kb_migrate_format` is
 * deferred by DR-22, so the choice really is permanent in this build and the UI
 * is obliged to say so. When migration ships the banner is deleted and nothing
 * else in this surface changes.
 *
 * This is also the surface a future `Convert format` action reuses. Do not build
 * a second one.
 */
export function KbFormatChooser({ open, onOpenChange, initialName = '', onCreate }: Props) {
  const [name, setName] = useState(initialName);
  const [format, setFormat] = useState<KbFormat>('okf');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setName(initialName);
    setFormat('okf');
    setBusy(false);
    setError(null);
  }, [open, initialName]);

  const id = useMemo(() => previewKbId(name), [name]);

  async function submit() {
    const trimmed = name.trim();
    if (!trimmed) {
      setError('Enter a name.');
      return;
    }
    if (!previewKbId(trimmed)) {
      setError('Choose a name with letters or numbers.');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onCreate(trimmed, format);
      onOpenChange(false);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <ModalShell
      open={open}
      onOpenChange={(next) => {
        if (!next && busy) return;
        onOpenChange(next);
      }}
      size="md"
      purpose={busy ? 'required' : 'form'}
      title="Create knowledge base"
      subtitle="Pick a name and the format its pages are written in."
      footer={
        <>
          <Button
            type="button"
            variant="secondary"
            onClick={() => onOpenChange(false)}
            disabled={busy}
          >
            Cancel
          </Button>
          <Button
            type="button"
            data-testid="knowledge-format-submit"
            onClick={() => void submit()}
            disabled={busy}
          >
            {busy ? 'Creating…' : 'Create knowledge base'}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4 pb-1">
        <div>
          <label className="mb-1 block text-label text-text-default" htmlFor="kb-format-name">
            Name
          </label>
          <Input
            id="kb-format-name"
            data-testid="knowledge-format-name"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                void submit();
              }
            }}
            placeholder="Knowledge base name"
            autoFocus
          />
          <p className="mt-1 text-supporting text-text-muted">
            Will be created as <span className="font-mono">knowledge/{id || '…'}/</span> — the final
            id may differ if that name is taken.
          </p>
        </div>

        <div role="radiogroup" aria-label="Knowledge base format" className="flex flex-col">
          {FORMATS.map((entry) => (
            <div key={entry.value}>
              <CustomRadio
                id={`kb-format-${entry.value}`}
                name="kb-format"
                value={entry.value}
                checked={format === entry.value}
                onChange={() => setFormat(entry.value)}
                label={
                  <span className="flex items-center gap-2">
                    {entry.name}
                    <Badge uppercase>{entry.code}</Badge>
                  </span>
                }
                secondaryLabel={entry.when}
              />
              {/* A plain `<ul>` with no marks. The draft gave each item a 4px
                  `--radius-full` dot; A-04 permits `--radius-full` on exactly
                  three things — status dots, the switch knob, avatars — and a
                  list bullet is none of them, and 4px is below the 8px diameter
                  §4.2 fixes for the dot object. */}
              <ul className="ml-8 list-none space-y-1 pb-2 text-supporting text-text-muted">
                {entry.facts.map((fact) => (
                  <li key={fact}>{fact}</li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <div className="flex items-start gap-2 rounded-element bg-wash-warning p-3">
          <AlertTriangle
            aria-hidden="true"
            className="mt-px h-icon-banner w-icon-banner shrink-0 text-text-warning"
          />
          <p className="text-supporting text-text-default">
            You cannot change a knowledge base&rsquo;s format yet. Pick BioOKF only if you want the
            biomedical vocabulary enforced from the first page.
          </p>
        </div>

        {error && (
          <p role="alert" className="text-body text-text-danger">
            {error}
          </p>
        )}
      </div>
    </ModalShell>
  );
}
