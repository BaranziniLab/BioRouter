export const STARTUP_UPDATE_CHECK_DELAY_MS = 5_000;
export const UPDATE_CHECK_INTERVAL_MS = 3 * 60 * 60 * 1_000;

export type AutomaticUpdateCheckReason = 'startup' | 'periodic';

export function scheduleUpdateChecks(
  runCheck: (reason: AutomaticUpdateCheckReason) => void | Promise<void>
): () => void {
  const startupTimer = setTimeout(() => {
    void runCheck('startup');
  }, STARTUP_UPDATE_CHECK_DELAY_MS);

  const periodicTimer = setInterval(() => {
    void runCheck('periodic');
  }, UPDATE_CHECK_INTERVAL_MS);

  return () => {
    clearTimeout(startupTimer);
    clearInterval(periodicTimer);
  };
}
