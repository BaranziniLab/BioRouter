export const SESSION_TOOLS_CHANGED_EVENT = 'session-tools:changed';
export const OPEN_CURRENT_CHAT_EXTENSIONS_EVENT = 'current-chat-extensions:open';

export type SessionToolEventDetail = { sessionId: string };

function dispatchSessionEvent(name: string, sessionId: string) {
  window.dispatchEvent(new CustomEvent<SessionToolEventDetail>(name, { detail: { sessionId } }));
}

export function notifySessionToolsChanged(sessionId: string) {
  dispatchSessionEvent(SESSION_TOOLS_CHANGED_EVENT, sessionId);
}

export function openCurrentChatExtensions(sessionId: string) {
  dispatchSessionEvent(OPEN_CURRENT_CHAT_EXTENSIONS_EVENT, sessionId);
}
