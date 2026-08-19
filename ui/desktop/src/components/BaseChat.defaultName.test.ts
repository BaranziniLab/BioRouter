import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const source = readFileSync(path.join(process.cwd(), 'src/components/BaseChat.tsx'), 'utf8');

// BaseChat's two fallback expressions live inside a component that requires the
// router, desktop bridge, chat contexts, stream registry, and settings providers.
// The reducer and stream fallback are exercised behaviorally in their own suites;
// this narrow source contract pins the two otherwise impractical integration sites.
describe('BaseChat default name integration', () => {
  it('publishes New chat through the ChatContext model before session metadata loads', () => {
    const chatModel = /const chat: ChatType = \{[\s\S]*?\n {2}\};/.exec(source);

    expect(chatModel, 'BaseChat no longer constructs the ChatContext model').not.toBeNull();
    expect(chatModel![0]).toMatch(/name: session\?\.name \|\| 'New chat'/);
  });

  it('shows New chat in the title pill before session metadata loads', () => {
    const titlePill = /<SessionNamePill\b[\s\S]*?\/>/.exec(source);

    expect(titlePill, 'BaseChat no longer renders SessionNamePill').not.toBeNull();
    expect(titlePill![0]).toMatch(/name=\{session\?\.name \|\| 'New chat'\}/);
  });
});
