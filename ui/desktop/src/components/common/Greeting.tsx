import { useState } from 'react';
import { useTextAnimator } from '../../hooks/use-text-animator';

interface GreetingProps {
  className?: string;
  /**
   * Set false where the heading should appear immediately with no unroll.
   * Defaults to animating, because every place this renders is an arrival:
   * a new chat, a new window, or Home.
   */
  animate?: boolean;
}

/**
 * The heading above the composer on an empty chat, and on Home.
 *
 * ⚠ **The rotation is deliberate product voice, not accidental filler.** These
 * are the stock sentences. I removed them once as marketing register and was
 * corrected: the variety is the point, and the operator wants a different line
 * on each arrival. Do not collapse this back to one fixed sentence.
 *
 * ⚠ **It unrolls on EVERY mount, on purpose.** `010bf68e` ("Keep chat greetings
 * still and immediate") removed the animator because `BaseChat` renders
 * `<Greeting key={sessionId}>` and every remount replayed it. A later attempt
 * gated it to once per chat. Both were wrong for the same reason: an arrival is
 * exactly when the unroll should play, and Home, a new window and a new chat
 * are all arrivals. The animator already honours `prefers-reduced-motion`,
 * which is the accessibility answer to "some people do not want motion" — a
 * blanket removal was not.
 */
const MESSAGES = [
  'What insights will your data reveal today?',
  'Which connections in the knowledge graph will lead to better care?',
  'What patient story will you uncover in the EHR today?',
  "Which patterns will the knowledge graph unlock for tomorrow's treatments?",
  'What unanswered question in the EHR can we tackle next?',
  "How will today's data bring us closer to a new breakthrough?",
  'Which patient trends are waiting to be discovered in the EHR?',
  'What surprising links might the knowledge graph reveal today?',
  "Which treatment paths can we refine from today's data?",
  'How will your next query shape patient outcomes?',
  'Which health discovery is hidden in your data today?',
  'What clinical journey will your analysis improve today?',
  'What relationships in the data will bring us closer to a cure?',
  'What question will your data answer next?',
  'Which medical mystery might the knowledge graph help solve today?',
] as const;

export function Greeting({
  className = 'mt-1 text-2xl font-semibold tracking-tight',
  animate = true,
}: GreetingProps) {
  // Chosen once per instance, in a lazy initialiser, so a re-render does not
  // swap the sentence out from under a running animation. A remount is a new
  // arrival and gets a new line, which is the intent.
  const [message] = useState(() => MESSAGES[Math.floor(Math.random() * MESSAGES.length)]);

  const messageRef = useTextAnimator({ text: message, enabled: animate });

  return (
    <h1 className={className}>
      <span ref={messageRef}>{message}</span>
    </h1>
  );
}
