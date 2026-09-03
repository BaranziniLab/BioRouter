import { describe, expect, it, vi } from 'vitest';
import { toolCountWarning } from './alerts/toolCountWarning';

describe('toolCountWarning', () => {
  it('accurately scopes the mixed count and reviews this chat extensions', () => {
    const open = vi.fn();
    window.addEventListener('current-chat-extensions:open', open);

    const warning = toolCountWarning(61, 'chat-1');
    expect(warning?.message).toContain('This chat can call 61 tools');
    expect(warning?.message).toContain('including built-in capabilities');
    expect(warning?.message).toContain('Capability defaults apply to a new chat');
    expect(warning?.action?.text).toBe('Review extensions');
    warning?.action?.onClick();

    expect(open).toHaveBeenCalledWith(expect.objectContaining({ detail: { sessionId: 'chat-1' } }));
    window.removeEventListener('current-chat-extensions:open', open);
  });

  it('does not offer chat management without an active chat', () => {
    expect(toolCountWarning(61, null)).toBeNull();
  });
});
