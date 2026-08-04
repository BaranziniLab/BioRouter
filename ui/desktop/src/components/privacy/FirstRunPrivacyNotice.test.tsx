import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';

/**
 * Task 38 (issue #56 §15.5) — the day-one notice.
 *
 * The property under test is the one the plan singles out: the notice quotes the
 * **History-visible** count, not the raw `sessions` row count. On the operator's
 * machine those differ by nearly 2× (498 visible against 936 raw), and quoting
 * the raw one tells the user to go and act on hundreds of conversations that are
 * not in their window.
 *
 * The visibility filter itself lives in SQL — `list_sessions_by_types` INNER
 * JOINs `messages` — so `GET /sessions` never hands the renderer an invisible
 * row at all. That is exactly why the fixture below contains none: a fixture
 * with message-less rows in it would be testing a filter this layer does not
 * own, and would pass whether or not the daemon applied it.
 */
const mocks = vi.hoisted(() => ({
  refreshSessionList: vi.fn(),
}));

vi.mock('../../utils/sessionListCache', () => ({
  refreshSessionList: mocks.refreshSessionList,
}));

import {
  computeNoticeCounts,
  FirstRunPrivacyNotice,
  providerLabel,
  shouldShowFirstRunNotice,
  UNKNOWN_PROVIDER,
} from './FirstRunPrivacyNotice';
import type { Session } from '../../api';

/** A row shaped the way `GET /sessions` serves one. */
function row(
  id: string,
  tier: 'private' | 'public' | undefined,
  provider: string | null,
  reason?: string
): Session {
  return {
    id,
    name: id,
    description: '',
    working_dir: '/tmp',
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    extension_data: {} as Session['extension_data'],
    message_count: 1,
    privacy_tier: tier,
    privacy_reason: reason ?? null,
    provider_name: provider,
  } as Session;
}

/**
 * The same eight-session shape the Rust test uses, minus the one row the daemon
 * would never send — so the two implementations are checked against one fixture.
 */
const FIXTURE: Session[] = [
  row('s1', 'private', 'versa_azure', 'backfill:versa_azure'),
  row('s2', 'private', 'versa_azure', 'backfill:versa_azure'),
  row('s4', 'public', 'anthropic'),
  row('s5', 'public', 'anthropic'),
  row('s6', 'public', 'anthropic'),
  row('s7', 'public', null),
  row('s8', 'public', null),
];

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('computeNoticeCounts', () => {
  it('quotes the History-visible count, not the raw one', () => {
    const counts = computeNoticeCounts(FIXTURE);
    expect(counts.privateVisible).toBe(2);
    expect(counts.publicNamedVisible).toBe(3);
    expect(counts.unknownProviderVisible).toBe(2);
    expect(counts.totalVisible).toBe(7);
  });

  it('groups the private rows by the provider the migration read', () => {
    const counts = computeNoticeCounts([
      ...FIXTURE,
      row('s9', 'private', 'ollama', 'backfill:ollama'),
      row('s10', 'private', 'llamacpp', 'backfill:llamacpp'),
      row('s11', 'private', 'llamacpp', 'backfill:llamacpp'),
    ]);
    expect(counts.privateByProvider).toEqual([
      { provider: 'llamacpp', count: 2 },
      { provider: 'versa_azure', count: 2 },
      { provider: 'ollama', count: 1 },
    ]);
  });

  it('reads a private row whose provenance is not a backfill from its bound provider', () => {
    // A chat raised by Gate C carries `mcp:<extension>`, not `backfill:*`. It is
    // still private and still belongs in the breakdown.
    const counts = computeNoticeCounts([row('s1', 'private', 'versa_azure', 'mcp:ucsfomopagent')]);
    expect(counts.privateByProvider).toEqual([{ provider: 'versa_azure', count: 1 }]);
  });

  it('files a private row with no provenance at all under the unknown bucket', () => {
    const counts = computeNoticeCounts([row('s1', 'private', null)]);
    expect(counts.privateByProvider).toEqual([{ provider: UNKNOWN_PROVIDER, count: 1 }]);
    expect(providerLabel(UNKNOWN_PROVIDER)).toBe('Provider not recorded');
  });

  it('fails closed on a tier it cannot read', () => {
    // A daemon too old to send the column, or a projection that dropped it.
    // `PrivacyBadge` will paint these rows Private; a notice that counted them
    // public would quote a friendlier number than the badges beside it.
    const counts = computeNoticeCounts([row('s1', undefined, 'anthropic')]);
    expect(counts.privateVisible).toBe(1);
    expect(counts.publicNamedVisible).toBe(0);
  });

  it('passes an unrecognised provider id through rather than dropping it', () => {
    expect(providerLabel('some_future_provider')).toBe('some_future_provider');
  });
});

describe('shouldShowFirstRunNotice', () => {
  it('is false when nothing visible came out private', () => {
    expect(shouldShowFirstRunNotice(computeNoticeCounts([row('s1', 'public', 'anthropic')]))).toBe(
      false
    );
  });

  it('is true as soon as one visible chat was marked', () => {
    expect(shouldShowFirstRunNotice(computeNoticeCounts(FIXTURE))).toBe(true);
  });
});

describe('FirstRunPrivacyNotice', () => {
  it('computes its numbers from the user own chat list', async () => {
    mocks.refreshSessionList.mockResolvedValue(FIXTURE);
    render(<FirstRunPrivacyNotice open onDismiss={vi.fn()} />);

    const headline = await screen.findByTestId('notice-headline');
    expect(headline.textContent).toContain('2');
    expect(headline.textContent).toContain('7');
    // The chats it will not vouch for are stated, not buried.
    expect(screen.getByTestId('notice-unknown').textContent).toContain('2');
    // ...and it asked for the History population, not the one with subagents.
    expect(mocks.refreshSessionList).toHaveBeenCalledWith(false);
  });

  it('lists the private chats by the model that marked them', async () => {
    mocks.refreshSessionList.mockResolvedValue(FIXTURE);
    render(<FirstRunPrivacyNotice open onDismiss={vi.fn()} />);

    const list = await screen.findByTestId('notice-by-provider');
    expect(within(list).getByText(/Versa \(Azure\) — 2/)).toBeTruthy();
  });

  it('says the tier came from the last model used, and how to repair it', () => {
    render(
      <FirstRunPrivacyNotice open onDismiss={vi.fn()} counts={computeNoticeCounts(FIXTURE)} />
    );
    const caveat = screen.getByTestId('notice-last-model-caveat').textContent ?? '';
    expect(caveat).toContain('last using');
    expect(caveat).toContain('switch it to a private model');
  });

  it('names the knowledge-base control, not only the exposure', () => {
    render(
      <FirstRunPrivacyNotice open onDismiss={vi.fn()} counts={computeNoticeCounts(FIXTURE)} />
    );
    const kb = screen.getByTestId('notice-knowledge-bases').textContent ?? '';
    expect(kb).toContain('start public');
    expect(kb).toContain('mark it private yourself');
  });

  it('admits it could not count rather than reporting zero', async () => {
    mocks.refreshSessionList.mockRejectedValue(new Error('daemon down'));
    render(<FirstRunPrivacyNotice open onDismiss={vi.fn()} />);

    await screen.findByTestId('notice-count-error');
    expect(screen.queryByTestId('notice-headline')).toBeNull();
  });

  it('does not read the chat list until it is opened', () => {
    render(<FirstRunPrivacyNotice open={false} onDismiss={vi.fn()} />);
    expect(mocks.refreshSessionList).not.toHaveBeenCalled();
  });

  it('acknowledging closes it', async () => {
    const user = userEvent.setup();
    const onDismiss = vi.fn();
    mocks.refreshSessionList.mockResolvedValue(FIXTURE);
    render(<FirstRunPrivacyNotice open onDismiss={onDismiss} />);

    await user.click(await screen.findByTestId('notice-acknowledge'));
    await waitFor(() => expect(onDismiss).toHaveBeenCalledTimes(1));
  });
});
