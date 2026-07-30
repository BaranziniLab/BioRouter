// Browser harness for the `<biorouter-ref …>` chip (issue #65).
//
// Mounts the REAL `ResourceRefChip` / `ResourceRefText` and the REAL
// `UserMessage` (transcript bubble and edit box, rail included) against the
// real stylesheet, so what renders here is what renders in the app. jsdom
// applies no Tailwind and computes no layout, so the unit tests can only assert
// the class contract; whether the chip reads as a first-class object, follows
// all three theme families in both modes, and keeps a hostile name inside its
// container is a question only a browser answers.
//
//   npx vite --config .reference-chip-harness/vite.config.mts --port 5201
//
// Driven by agent-browser: `?theme=<family>&mode=<light|dark>` selects a
// combination, and every combination is reachable from the toolbar.
import { StrictMode, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import UserMessage from '../src/components/UserMessage';
import { ResourceRefChip, ResourceRefText } from '../src/components/ResourceRefChip';
import { labelledRefTag, refTag } from '../src/utils/resourceRefs';
import { splitComposerText } from '../src/utils/composerRefs';
import type { Message } from '../src/api';
import './harness.css';

// The main process supplies these in the app; the components under test only
// use `logInfo`.
Object.assign(window, { electron: { logInfo: () => {} } });

const FAMILIES = ['parchment', 'alma-mater', 'roche-limit'] as const;
const MODES = ['light', 'dark'] as const;

// A name long enough to bleed past its container, plus one carrying every
// character the escape table exists for.
const LONG_NAME =
  'single-cell-rna-sequencing-quality-control-doublet-removal-and-ambient-rna-correction-pipeline-v2';
const HOSTILE_NAME = 'single-cell "QC" & prep <v2>';

const userMessage = (text: string): Message =>
  ({
    id: 'message-1',
    role: 'user',
    // `created` is seconds; passing milliseconds prints a year-58548 timestamp
    // that reads as a product bug in a screenshot.
    created: Math.floor(Date.now() / 1000),
    content: [{ type: 'text', text }],
    metadata: { userVisible: true, agentVisible: true },
  }) as Message;

/** The composer's reference rail, rendered from the real component. */
function ComposerRail({ text }: { text: string }) {
  const { body, refs } = splitComposerText(text);

  return (
    <div className="rounded-2xl border border-border-subtle bg-background-default px-4 pt-3 pb-3 shadow-[var(--shadow-composer)]">
      {refs.length > 0 && (
        <div className="mb-1.5 flex flex-wrap items-center gap-1.5 px-1">
          {refs.map((ref) => (
            <ResourceRefChip key={`${ref.kind}:${ref.value}`} refSpan={ref} onRemove={() => {}} />
          ))}
        </div>
      )}
      <div className="px-3 pt-3 pb-1.5 text-sm text-text-default">
        {body || <span className="text-text-muted">What can Biorouter help with?</span>}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-8">
      <h2 className="mb-2 text-xs font-semibold uppercase tracking-wider text-text-muted">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Harness() {
  const params = new URLSearchParams(window.location.search);
  const [family, setFamily] = useState<string>(params.get('theme') ?? FAMILIES[0]);
  const [mode, setMode] = useState<string>(params.get('mode') ?? 'light');

  useEffect(() => {
    document.documentElement.setAttribute('data-theme', family);
    document.documentElement.classList.toggle('dark', mode === 'dark');
  }, [family, mode]);

  return (
    <div className="min-h-screen bg-background-app p-6 font-sans">
      <div className="mx-auto max-w-3xl">
        <div className="mb-6 flex flex-wrap items-center gap-2">
          {FAMILIES.map((id) => (
            <button
              key={id}
              data-testid={`family-${id}`}
              onClick={() => setFamily(id)}
              className={`rounded-md border px-2 py-1 text-xs ${
                family === id
                  ? 'border-border-strong bg-background-medium text-text-default'
                  : 'border-border-subtle text-text-muted'
              }`}
            >
              {id}
            </button>
          ))}
          {MODES.map((id) => (
            <button
              key={id}
              data-testid={`mode-${id}`}
              onClick={() => setMode(id)}
              className={`rounded-md border px-2 py-1 text-xs ${
                mode === id
                  ? 'border-border-strong bg-background-medium text-text-default'
                  : 'border-border-subtle text-text-muted'
              }`}
            >
              {id}
            </button>
          ))}
          <span data-testid="current" className="ml-2 text-xs text-text-muted">
            {family} · {mode}
          </span>
        </div>

        <Section title="Chip, one per kind">
          <div className="flex flex-wrap items-center gap-2">
            <ResourceRefChip refSpan={{ kind: 'skill', value: 'single-cell' }} />
            <ResourceRefChip refSpan={{ kind: 'extension', value: 'Chat Recall' }} />
            <ResourceRefChip
              refSpan={{ kind: 'knowledge_base', value: 'soul', label: 'Soul & Body' }}
            />
            <ResourceRefChip refSpan={{ kind: 'skill', value: HOSTILE_NAME }} />
          </div>
        </Section>

        <Section title="Composer rail — removable">
          <ComposerRail
            text={`compare these cohorts ${refTag('skill', 'my skill')} ${refTag(
              'extension',
              'Chat Recall'
            )} ${labelledRefTag('knowledge_base', 'soul', 'Soul & Body')}`}
          />
        </Section>

        <Section title="Composer rail — a name long enough to break the layout">
          <ComposerRail text={refTag('skill', LONG_NAME)} />
        </Section>

        <Section title="Composer rail — narrow (320px)">
          <div className="w-[320px]">
            <ComposerRail
              text={`${refTag('skill', LONG_NAME)} ${refTag('extension', 'Chat Recall')}`}
            />
          </div>
        </Section>

        <Section title="Transcript — the real UserMessage">
          <UserMessage
            message={userMessage(
              `please run ${refTag('skill', 'my skill')} over the samples in the ${labelledRefTag(
                'knowledge_base',
                'soul',
                'Soul & Body'
              )} base and summarise`
            )}
          />
          <UserMessage message={userMessage(`one long one ${refTag('skill', LONG_NAME)}`)} />
          <UserMessage
            message={userMessage(
              `a tag this build cannot read <biorouter-ref type="skill" name="never closed`
            )}
          />
        </Section>

        <Section title="Inline in prose">
          <p className="text-sm leading-relaxed text-text-default">
            <ResourceRefText
              text={`Mixed into a sentence, a chip sits ${refTag(
                'skill',
                'my skill'
              )} on the baseline next to ${refTag('extension', 'Chat Recall')} the words around it.`}
            />
          </p>
        </Section>
      </div>
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Harness />
  </StrictMode>
);
