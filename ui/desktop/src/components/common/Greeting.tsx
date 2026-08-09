interface GreetingProps {
  className?: string;
  forceRefresh?: boolean;
}

/**
 * The line above the composer on an empty Home.
 *
 * ⚠ **One fixed, plain sentence. Do not restore the rotation.** This used to
 * pick at random from fifteen variants in the register of a product page:
 * "Which patterns will the knowledge graph unlock for tomorrow's treatments?",
 * "How will today's data bring us closer to a new breakthrough?", "Which
 * medical mystery might the knowledge graph help solve today?". It was the
 * largest concentration of marketing voice in the app, and it sat on the first
 * screen, which is the worst place for it: a clinical instrument that opens by
 * enthusing about cures reads as a brochure rather than a tool.
 *
 * It also promised things the app does not know about. Every variant assumed a
 * knowledge graph or an EHR, so a user doing none of that was greeted with
 * someone else's use case.
 *
 * A fixed line has a second benefit worth keeping: the old version called
 * `Math.random()` in a lazy initialiser on every mount, so the heading changed
 * under the user whenever the component remounted.
 */
export function Greeting({
  className = 'mt-1 text-2xl font-semibold tracking-tight',
  forceRefresh = false,
}: GreetingProps) {
  return (
    <h1 className={className} key={forceRefresh ? 'refresh' : undefined}>
      <span>What do you want to work on?</span>
    </h1>
  );
}
