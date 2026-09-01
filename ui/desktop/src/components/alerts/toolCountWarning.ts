import { openCurrentChatExtensions } from '../../utils/sessionToolEvents';
import { AlertType, type Alert } from './types';

const TOOLS_MAX_SUGGESTED = 60;

export function toolCountWarning(toolCount: number | null, sessionId: string | null): Alert | null {
  if (toolCount === null || toolCount <= TOOLS_MAX_SUGGESTED || !sessionId) return null;
  return {
    type: AlertType.Warning,
    message: `Too many tools can degrade performance.\nThis chat can call ${toolCount} tools, including built-in capabilities (recommend: ${TOOLS_MAX_SUGGESTED}). Review currently attached extensions here. Capability defaults apply to a new chat.`,
    action: {
      text: 'Review extensions',
      onClick: () => {
        window.dispatchEvent(new CustomEvent('hide-alert-popover'));
        openCurrentChatExtensions(sessionId);
      },
    },
    autoShow: false,
  };
}
