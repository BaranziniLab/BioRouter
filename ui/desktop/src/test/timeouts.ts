/**
 * The two test timeouts, in ONE place, because they are coupled and were not.
 *
 * There are two independent clocks in every React test here:
 *
 *   testTimeout       vitest's budget for the whole test function
 *   asyncUtilTimeout  Testing Library's budget for a single findBy or waitFor
 *
 * They must satisfy `asyncUtilTimeout < testTimeout`, with real headroom. When
 * they are equal — which is exactly what happened — a `waitFor` that uses its
 * full budget can never report its own error, because vitest kills the test at
 * the same instant. You lose "Unable to find an element with the text: …" and
 * get a bare "Test timed out in 5000ms" instead, which says nothing about what
 * the component failed to render. Worse, any test whose setup plus wait exceeds
 * the shared number fails even when the wait itself succeeded.
 *
 * How the repo arrived there is worth recording, because both steps were
 * locally reasonable:
 *
 *   1. asyncUtilTimeout defaults to 1000ms and is INDEPENDENT of testTimeout,
 *      so suites could not be stabilised by raising --testTimeout: the test had
 *      30s and findByText still gave up after 1s. It was raised to 5000ms.
 *   2. vitest.config.ts never set testTimeout, so it stayed at vitest's own
 *      default — also 5000ms. The two silently collided.
 *
 * The symptom was a suite that failed differently on every run (9, then 1, then
 * 0, then 18 files) and passed in isolation, which reads exactly like machine
 * contention and was misdiagnosed as such more than once. It is not contention:
 * a single file run ALONE fails at 5000 and passes at 30000, same duration.
 *
 * Both values are now set from here — vitest.config.ts reads TEST_TIMEOUT_MS,
 * setup.ts reads ASYNC_UTIL_TIMEOUT_MS — and setup.ts asserts the invariant at
 * load, so the two can never drift back into collision silently.
 */

/**
 * Whole-test budget. Generous on purpose: raising it costs nothing on a green
 * run, because it only bounds failures. It is NOT a substitute for fixing a
 * genuinely slow test — see the note on user-event below.
 */
export const TEST_TIMEOUT_MS = 30_000;

/** Same budget for beforeEach/afterEach and friends. */
export const HOOK_TIMEOUT_MS = 30_000;

/**
 * Single-wait budget. Must stay comfortably under TEST_TIMEOUT_MS so that a
 * wait which exhausts itself still leaves room for Testing Library to throw its
 * own, far more useful error.
 *
 * 5s is a load allowance, not a correctness change: a test asserting something
 * that never appears still fails, just later.
 */
export const ASYNC_UTIL_TIMEOUT_MS = 5_000;

/**
 * The headroom the invariant requires. A wait must be able to fail AND report,
 * with room for render and assertions either side of it.
 */
export const MIN_TIMEOUT_HEADROOM = 2;

/**
 * NOTE FOR SLOW TESTS: a per-test inline timeout — `it('…', fn, 60000)` —
 * OVERRIDES both the config and any --testTimeout CLI flag. That is a foot-gun:
 * one such line made CI red regardless of the flag it was passed. If a test
 * needs more than TEST_TIMEOUT_MS, prefer fixing the cause. The usual cause is
 * user-event's default inter-event delay on long typing; `userEvent.setup({
 * delay: null })` removes it.
 */
