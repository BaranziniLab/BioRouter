import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { CrossAffiliationAcceptCard } from './CrossAffiliationAcceptCard';
// The real constant, through the partial mock's `importOriginal` below — so the
// scope test compares the card against the mirrored bytes rather than against a
// second hand-typed copy, which would drift in exactly the direction the mirror
// exists to catch.
import { GRANT_SCOPE_COPY } from '../../utils/crossAffiliation';

const mockReadMixingMode = vi.fn();
const mockAccept = vi.fn();

vi.mock('../../utils/crossAffiliation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../utils/crossAffiliation')>()),
  readMixingMode: () => mockReadMixingMode(),
  acceptCrossAffiliationFlow: (...args: unknown[]) => mockAccept(...args),
}));

const ACCEPTED =
  'Cross-institutional data flow. The extension `ucsfomopagent` holds data belonging to UCSF, ' +
  'but this chat is bound to a model covered by Stanford’s agreements. Approving records your ' +
  'acceptance for this chat, this extension and this model’s institution only.';

beforeEach(() => {
  vi.clearAllMocks();
  mockReadMixingMode.mockResolvedValue('standard');
  mockAccept.mockResolvedValue(ACCEPTED);
});

const renderCard = (props: Partial<{ sessionId: string; extension: string }> = {}) =>
  render(
    <CrossAffiliationAcceptCard
      sessionId={props.sessionId ?? 'chat_7'}
      extension={props.extension ?? 'ucsfomopagent'}
    />
  );

describe('CrossAffiliationAcceptCard', () => {
  it('records the acceptance for the refused triple and shows the daemon’s own statement', async () => {
    const user = userEvent.setup();
    renderCard();

    const button = await screen.findByRole('button', { name: /approve this flow/i });
    await user.click(button);

    // Session and extension exactly as refused — the daemon reads the
    // institution off the model it samples, and the request carries none.
    expect(mockAccept).toHaveBeenCalledWith('chat_7', 'ucsfomopagent');

    // The confirmation is the daemon's composition, not a paraphrase: the
    // sentence recorded and the sentence read must not differ by a word.
    expect(await screen.findByText(ACCEPTED)).toBeInTheDocument();
    // …and the control is gone, so a second press cannot re-post it.
    expect(screen.queryByRole('button', { name: /approve this flow/i })).not.toBeInTheDocument();
    // The refusal told the model not to retry, and it will not. A recorded
    // acceptance beside a conversation that has stopped is the same dead end
    // one press further along, so the card says who moves next.
    expect(screen.getByTestId('cross-affiliation-accepted')).toHaveTextContent(
      /try that step again/i
    );
  });

  it('says what failed and leaves the control pressable when the daemon refuses', async () => {
    const user = userEvent.setup();
    // The realistic failure: a daemon the app did not start, holding no
    // user-action key (open question 23).
    mockAccept.mockRejectedValue('This backend was started without a user-action key.');
    renderCard();

    await user.click(await screen.findByRole('button', { name: /approve this flow/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/user-action key/i);
    // Nothing was recorded, so the way forward must still be on screen.
    expect(screen.getByRole('button', { name: /approve this flow/i })).toBeInTheDocument();
  });

  it('renders nothing under the open mixing policy', async () => {
    mockReadMixingMode.mockResolvedValue('open');
    const { container } = renderCard();

    // Settled — the mode really was read and answered, so this is not just the
    // pre-read blank.
    await waitFor(() => expect(mockReadMixingMode).toHaveBeenCalled());
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(mockAccept).not.toHaveBeenCalled();
  });

  it('offers no in-app acceptance under strict, and says why', async () => {
    mockReadMixingMode.mockResolvedValue('strict');
    renderCard();

    // Strict requires DR-20's system-level prompt on top of the in-app proof,
    // and nothing in this build can raise one. Accepting on the weaker proof
    // would be the control quietly downgrading itself, so it fails closed.
    expect(await screen.findByTestId('cross-affiliation-strict')).toHaveTextContent(
      /system password/i
    );
    expect(screen.queryByRole('button', { name: /approve this flow/i })).not.toBeInTheDocument();
    expect(mockAccept).not.toHaveBeenCalled();
  });

  it('offers nothing when there is no chat to key the acceptance on', async () => {
    const { container } = render(
      <CrossAffiliationAcceptCard sessionId={undefined} extension="ucsfomopagent" />
    );
    await waitFor(() => expect(container).toBeEmptyDOMElement());
    expect(mockAccept).not.toHaveBeenCalled();
  });

  it('states the scope before the user presses, not only after', async () => {
    renderCard();
    await screen.findByRole('button', { name: /approve this flow/i });
    // "How far does my yes reach" is part of what is being decided.
    //
    // ⚠ The WHOLE sentence, byte for byte, and not three regexes for its three
    // narrowings. Review found the seam: `privacy::grant::tests::
    // the_scope_copy_the_user_reads_is_the_one_the_daemon_records` asserts the
    // constant exists in the module FILE, and a loose match here asserts the
    // card says something like it — so a card that kept the export and rendered
    // a paraphrase satisfied both, and the byte-level mirror the Rust test
    // exists to enforce stopped reaching the screen. This closes it: the two
    // tests now pin the same bytes at both ends of the wire.
    expect(screen.getByTestId('cross-affiliation-accept')).toHaveTextContent(GRANT_SCOPE_COPY);
  });
});
