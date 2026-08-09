import type { FC } from 'react';
import type { LucideProps } from 'lucide-react';
import {
  AppWindow,
  Bot,
  CalendarClock,
  GitBranch,
  MessageSquare,
  MessageSquareLock,
  Terminal,
} from '../icons/app-icons';
import type { SessionClassification } from '../../api/types.gen';

/**
 * What KIND of chat a row represents. One axis, decided from the session record.
 *
 * ⚠ **Kind is not privacy.** A chat has both — a delegated sub-agent can be
 * private, a branch can be public — so the two are resolved separately and
 * composed by {@link chatIconFor}. Collapsing them into one enum is how you end
 * up unable to say "private branch".
 */
export type ChatKind = 'chat' | 'branch' | 'subagent' | 'app' | 'scheduled' | 'terminal';

/**
 * The fields any chat-listing surface has. Deliberately structural rather than
 * the generated `Session`: the sidebar's `SessionSummary`, History's rows and
 * the tab strip's tabs are three different shapes that all carry these, and a
 * surface that only knows a name still gets a correct answer for `app` and the
 * legacy branch case.
 */
export interface ChatKindSource {
  name?: string | null;
  /** BR-45: set by `diverge_session`. The durable mark of a user fork. */
  diverged_from?: string | null;
  /** BR-71: the delegating parent, set when this session is a sub-agent. */
  parent_session_id?: string | null;
  /** As stored: `user` / `scheduled` / `sub_agent` / `hidden` / `terminal`. */
  session_type?: string | null;
}

/**
 * ⚠ **Order matters, and it is not arbitrary.** A sub-agent that was itself
 * diverged carries both `parent_session_id` and `diverged_from`; the delegation
 * is the more consequential fact about it (it is not a chat the user is holding),
 * so it wins. `app` is checked first because an app's chat is not a chat the
 * user opens at all.
 */
export function chatKindOf(session: ChatKindSource): ChatKind {
  const name = (session.name ?? '').trim();
  if (name.startsWith('app:')) return 'app';

  const type = session.session_type ?? null;
  if (type === 'terminal') return 'terminal';
  if (type === 'sub_agent' || session.parent_session_id) return 'subagent';
  if (type === 'scheduled') return 'scheduled';

  // `diverged_from` is the durable signal. The name regex behind it is the
  // legacy fallback and nothing more: it was the ONLY signal before BR-45
  // recorded lineage, so rows written then still need to read as branches — but
  // it is also defeated by anyone who renames a branch, which is precisely why
  // it stopped being the primary test.
  if (session.diverged_from) return 'branch';
  if (/\(branch \d+\)$/i.test(name)) return 'branch';

  return 'chat';
}

interface ChatIcon {
  Icon: FC<LucideProps>;
  /** Screen-reader text. Always states the kind AND, when private, the tier. */
  label: string;
}

/**
 * The glyph for one chat, from its kind and its privacy tier.
 *
 * ⚠ **Privacy is carried by the glyph itself for a plain chat, and by the
 * accessible name for every kind.** Replacing the dense dot with a hue alone
 * would have made a safety-relevant marking invisible to anyone who cannot
 * separate the two inks, so `private` swaps the bubble for a padlocked bubble —
 * a shape difference, not a colour one — and the label says the word regardless
 * of kind. The remaining kinds keep their own shape (a private branch is still
 * more usefully a branch than a padlock) and rely on the label plus the tier
 * ink the call site applies.
 */
export function chatIconFor(kind: ChatKind, tier?: SessionClassification | null): ChatIcon {
  const isPrivate = tier === 'private';
  const privateSuffix = isPrivate ? ', private' : '';

  switch (kind) {
    case 'app':
      return { Icon: AppWindow, label: `App${privateSuffix}` };
    case 'terminal':
      return { Icon: Terminal, label: `Terminal${privateSuffix}` };
    case 'subagent':
      return { Icon: Bot, label: `Sub-agent${privateSuffix}` };
    case 'scheduled':
      return { Icon: CalendarClock, label: `Scheduled run${privateSuffix}` };
    case 'branch':
      return { Icon: GitBranch, label: `Branched chat${privateSuffix}` };
    case 'chat':
    default:
      return isPrivate
        ? { Icon: MessageSquareLock, label: 'Private chat' }
        : { Icon: MessageSquare, label: 'Chat' };
  }
}
