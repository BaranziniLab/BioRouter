/**
 * Issue #56 DR-16: the header that proves a request came from the person at the
 * keyboard rather than from the model.
 *
 * The daemon cannot tell the two apart on its own — `check_token` compares one
 * machine-wide bearer and has no principal, so every authenticated request looks
 * identical whoever sent it (AR-11/AR-15). Three routes therefore require this
 * header before they will raise a chat's privacy capability:
 * `/agent/update_provider`, `/config/set_provider`, and `/config/upsert` for the
 * handful of keys that decide what capability new chats start at.
 *
 * ⚠ It is attached PER REQUEST, never through `client.setConfig`. A default
 * header rides on every call; this one rides on the three that need it, which is
 * what keeps the proof narrower than the daemon secret rather than a second copy
 * of it.
 *
 * The renderer is the user's surface, so every call it makes is a user act. The
 * model reaches these same routes over HTTP without going through here, and that
 * is precisely the caller the header separates out.
 */
export const userActionHeaders = async (): Promise<Record<string, string>> => {
  try {
    return { 'X-User-Action': await window.electron.getUserActionKey() };
  } catch {
    // An older preload, or a surface with no bridge at all. Sending nothing
    // fails closed at the daemon, which is the correct direction: the request
    // is refused and explained, not silently allowed.
    return {};
  }
};
