import { cn } from '../utils';

/**
 * The landing state's suggestion chips.
 *
 * A new chat opened onto a greeting and an empty composer, which answers "who
 * are you" and never answers "what can I ask you". This is the one place in the
 * app where that affordance genuinely belongs: a transcript with even one
 * message in it has already told the user what the conversation is for, and a
 * row of suggestions there would be the interface talking over them.
 *
 * Two rules the chips are built to:
 *
 * - **They FILL the composer, they do not SEND.** A suggestion is a starting
 *   sentence the user edits — usually to name the file, the topic or the
 *   question they actually have. A chip that fired a turn would spend a model
 *   call on a prompt nobody meant literally, and there is no way to take it
 *   back.
 * - **They are the quietest controls on the screen.** Ghost, hairline, muted
 *   ink, one step below the composer they sit under. They are an answer to a
 *   question the user may not have asked, so they must never compete with the
 *   composer for the first click.
 */
interface Suggestion {
  /** What the chip says. Short — it is scanned, not read. */
  label: string;
  /** What lands in the composer. A complete sentence the user can edit or send. */
  prompt: string;
}

/**
 * Deliberately generic and setup-free: each one works against whatever working
 * directory the session already has, and none of them promises a capability
 * that depends on an extension being installed.
 */
const SUGGESTIONS: Suggestion[] = [
  {
    label: 'Explore my data',
    prompt: "Look at the data files in my working directory and tell me what's in them.",
  },
  {
    label: 'Orient me in this project',
    prompt:
      'Give me an orientation to this project — what it does, how it is laid out, and where to start reading.',
  },
  {
    label: 'Make a figure',
    prompt: 'Make a chart from one of the data files in my working directory.',
  },
  {
    label: 'Write a script',
    prompt: "Write and run a small script for me. I'll describe what it should do.",
  },
];

interface ComposerSuggestionsProps {
  /** Scopes the insert to this chat's composer; `null` is the pre-session one. */
  sessionId: string | null;
  /** Suppressed when something has already put text in the composer. */
  hidden?: boolean;
  className?: string;
}

export function ComposerSuggestions({ sessionId, hidden, className }: ComposerSuggestionsProps) {
  if (hidden) return null;

  const insert = (prompt: string) => {
    window.dispatchEvent(
      new CustomEvent('insert-chat-input', {
        detail: { sessionId: sessionId ?? null, value: prompt },
      })
    );
  };

  return (
    <div
      data-testid="composer-suggestions"
      role="group"
      aria-label="Suggested ways to start"
      className={cn('flex flex-wrap items-center justify-center gap-2', className)}
    >
      {SUGGESTIONS.map((suggestion) => (
        <button
          key={suggestion.label}
          type="button"
          onClick={() => insert(suggestion.prompt)}
          title={suggestion.prompt}
          className={cn(
            'flex h-control-sm items-center rounded-element border border-border-subtle px-3',
            'text-secondary text-text-muted tint-interactive transition-colors',
            'cursor-pointer hover:text-text-default'
          )}
        >
          {suggestion.label}
        </button>
      ))}
    </div>
  );
}

export default ComposerSuggestions;
