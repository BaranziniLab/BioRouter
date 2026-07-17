import { describe, expect, it } from 'vitest';
import { diagnosticsArchiveBytes, diagnosticsArchiveFilename } from './diagnosticsExport';

describe('diagnostics export', () => {
  it('builds a safe filename from the session id', () => {
    expect(diagnosticsArchiveFilename('20260716_27')).toBe('diagnostics_20260716_27.zip');
    expect(diagnosticsArchiveFilename('../../session\nname')).toBe('diagnostics_session_name.zip');
    expect(diagnosticsArchiveFilename('...')).toBe('diagnostics_session.zip');
  });

  it('accepts standard, empty, and spanned ZIP signatures', () => {
    expect(diagnosticsArchiveBytes(Uint8Array.from([0x50, 0x4b, 0x03, 0x04]))).toHaveLength(4);
    expect(diagnosticsArchiveBytes(Uint8Array.from([0x50, 0x4b, 0x05, 0x06]))).toHaveLength(4);
    expect(diagnosticsArchiveBytes(Uint8Array.from([0x50, 0x4b, 0x07, 0x08]))).toHaveLength(4);
  });

  it('rejects empty and non-ZIP responses', () => {
    expect(() => diagnosticsArchiveBytes(new ArrayBuffer(0))).toThrow('archive is empty');
    expect(() => diagnosticsArchiveBytes(Uint8Array.from([1, 2, 3, 4]))).toThrow(
      'not a valid ZIP archive'
    );
  });
});
