import { useRef } from 'react';
import { useTextAnimator } from '../../hooks/use-text-animator';

interface GreetingProps {
  className?: string;
  /**
   * The chat this greeting belongs to. The unroll plays ONCE per chat; pass the
   * session id so a remount of the same chat stays still. Omit it and the
   * greeting never animates, which is the right default for anywhere that is
   * not a new chat.
   */
  animateOnceFor?: string;
}

/**
 * The line above the composer on an empty chat.
 *
 * ⚠ **One fixed, plain sentence. Do not restore the rotation.** This used to
 * pick at random from fifteen variants in the register of a product page:
 * "Which patterns will the knowledge graph unlock for tomorrow's treatments?",
 * "How will today's data bring us closer to a new breakthrough?". It was the
 * largest concentration of marketing voice in the app and it sat on the first
 * screen. Every variant also assumed a knowledge graph or an EHR, so a user
 * doing neither was greeted with someone else's use case.
 *
 * ⚠ **The unroll plays on a NEW chat and nowhere else**, and that distinction
 * is the whole point. `010bf68e` ("Keep chat greetings still and immediate")
 * removed the animation outright because `BaseChat` renders
 * `<Greeting key={sessionId}>`, so EVERY remount replayed it: reopening a saved
 * chat, a renderer reload, switching back to a tab. An animation that fires
 * when nothing new happened reads as a glitch, which is what that commit was
 * reacting to.
 *
 * Removing it also removed the thing people liked on a genuinely new tab, so
 * the fix is the gate rather than the animation. `SEEN` records which chats
 * have already played; a second mount of the same id renders still. It is
 * deliberately module-level and NOT persisted: a full reload is a new app
 * session, and a greeting that unrolls once after a restart is correct.
 */
const SEEN = new Set<string>();

/** Test-only: forget which chats have played. */
export function resetGreetingAnimationForTests() {
  SEEN.clear();
}

export function Greeting({
  className = 'mt-1 text-2xl font-semibold tracking-tight',
  animateOnceFor,
}: GreetingProps) {
  // Decided once, on the first render of this instance. Reading it during
  // render rather than in an effect is deliberate: the animator needs to know
  // before it attaches, and an effect would let one unanimated frame paint.
  const shouldAnimate = useRef<boolean | null>(null);
  if (shouldAnimate.current === null) {
    const fresh = animateOnceFor !== undefined && !SEEN.has(animateOnceFor);
    if (fresh) SEEN.add(animateOnceFor);
    shouldAnimate.current = fresh;
  }

  const text = 'What do you want to work on?';
  const messageRef = useTextAnimator({
    text,
    enabled: shouldAnimate.current,
  });

  return (
    <h1 className={className}>
      <span ref={messageRef}>{text}</span>
    </h1>
  );
}
