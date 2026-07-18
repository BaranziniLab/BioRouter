import { UserAttachment } from '../types/message';

/**
 * The route-state contract for /pair.
 *
 * The `Pair` component that used to live here was a 34-line pass-through to
 * BaseChat, and it retired when /pair became tabbed: ChatGroupsShell now mounts
 * BaseChat per group, and the tab strip renders through BaseChat's own header
 * seam. Only the route-state shape survives, because the navigations that carry
 * it (Hub composer submit, diverge, the sidebar's new-chat and recents) are
 * unchanged — they remain the deep-link inbox the ChatGroups URL adapter reads.
 */
export interface PairRouteState {
  newChat?: boolean;
  resumeSessionId?: string;
  initialMessage?: string;
  initialAttachments?: UserAttachment[];
  /** Set by the sidebar's recents: a single click opens a preview tab. */
  preview?: boolean;
}
