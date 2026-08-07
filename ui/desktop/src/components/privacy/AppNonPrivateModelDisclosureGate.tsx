import { useModelAndProvider } from '../ModelAndProviderContext';
import { NonPrivateModelDisclosureGate } from './NonPrivateModelDisclosureGate';

/**
 * The install-wide mount of {@link NonPrivateModelDisclosureGate}, keyed on the
 * provider **the app is configured to use** rather than on any one chat's
 * (issue #56, DR-17 requirement 3).
 *
 * ⚠ **This is what makes "before the first turn" true.** The chat-level mount in
 * `BaseChat` reads `session?.provider_name`, and a session row exists only once
 * `/agent/resume` has one to return — which on a fresh install is only after the
 * first turn has been sent. So the chat-level gate alone disclosed *after* the
 * transcript went out: a receipt, which is exactly what Task 30A's own doc
 * comment says a disclosure must not become. This gate needs no session, no chat
 * and no route: `BIOROUTER_PROVIDER` is bound as soon as onboarding writes it,
 * and the dialog is modal, so the composer behind it cannot send.
 *
 * ⚠ **Both mounts, not one.** Removing the chat-level mount would miss a chat
 * bound to a public provider on a machine whose *default* is private — the
 * per-chat picker can differ from the app default, and the disclosure is about
 * the model the chat will actually talk to. `useSoleDisclosurePresenter` already
 * guarantees exactly one dialog however many gates want it, which is the same
 * mechanism that lets six split panes mount six gates.
 *
 * ⚠ **`currentProvider`, never `modelConfigStatus`.** A null provider means two
 * different things — "not read yet" and "nothing configured" — and the gate
 * treats null as *wait* in both, which is the correct answer to each: there is
 * nothing to disclose about a provider we have not resolved, and nothing to
 * disclose about one that does not exist. Consulting the status here would only
 * let a caller draw the conclusion the gate deliberately declines to draw.
 */
export function AppNonPrivateModelDisclosureGate() {
  const { currentProvider } = useModelAndProvider();
  return <NonPrivateModelDisclosureGate providerName={currentProvider} />;
}
