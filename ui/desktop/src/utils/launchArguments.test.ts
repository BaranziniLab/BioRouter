import { describe, expect, it } from 'vitest';
import { findBrxtArgument, isBrxtFile } from './launchArguments';

describe('Biorouter extension launch arguments', () => {
  it('finds case-insensitive .brxt paths with spaces', () => {
    expect(findBrxtArgument(['Biorouter.exe', 'C:\\My Extensions\\Study.BRXT'])).toBe(
      'C:\\My Extensions\\Study.BRXT'
    );
  });

  it('removes quotes retained by a launcher', () => {
    expect(findBrxtArgument(['Biorouter.exe', '"C:\\My Extensions\\Study.brxt"'])).toBe(
      'C:\\My Extensions\\Study.brxt'
    );
  });

  it('does not mistake a suffix after the extension for a bundle', () => {
    expect(findBrxtArgument(['Biorouter.exe', 'study.brxt.exe'])).toBeUndefined();
    expect(isBrxtFile('/tmp/study.brxt')).toBe(true);
  });
});
