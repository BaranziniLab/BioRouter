import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

describe('Capabilities settings copy', () => {
  it('states that capability switches are defaults for new chats', () => {
    const source = readFileSync(join(__dirname, 'ChatSettingsSection.tsx'), 'utf8');

    expect(source).toContain('new chats start with');
    expect(source).toContain('Existing chats keep their current');
  });
});
