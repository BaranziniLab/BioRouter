/**
 * launchDependencyDebug.ts
 *
 * The renderer half of "Debug with Biorouter": gather the environment, build the
 * briefing, and open it in a NEW chat window.
 *
 * A new window, never the current chat. The user hit this from an install that
 * failed — usually while in the middle of something else — and appending a wall
 * of shell output to whatever conversation they had open would hijack it. The
 * seeded message is auto-submitted into a fresh session (see
 * `utils/launcherMessage.ts`), so the agent is already working by the time the
 * window paints.
 */
import {
  buildDependencyDebugPrompt,
  type DependencyFailure,
  type DependencyEnvironment,
} from './dependencyDebugPrompt';

/**
 * Ask the main process what the app's environment actually looks like.
 * Never fatal: a briefing without the environment section is worth far more than
 * no briefing at all.
 */
async function collectEnvironment(): Promise<DependencyEnvironment | undefined> {
  try {
    return await window.electron.dependencyEnvironment();
  } catch {
    return undefined;
  }
}

export async function launchDependencyDebugSession(
  failure: Omit<DependencyFailure, 'environment'>
): Promise<void> {
  const environment = await collectEnvironment();
  const prompt = buildDependencyDebugPrompt({ ...failure, environment });
  window.electron.createChatWindow(prompt);
}
