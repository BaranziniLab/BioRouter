import { describe, expect, it } from 'vitest';
import { friendlyArtifactFileError } from './artifactFileErrors';

// #36 — the artifact panel must never surface a raw Node errno string
// ("ENOENT: no such file or directory, stat '/…'"); every recognised code maps
// to a sentence a person can act on, plus the structured code for the viewer.
describe('friendlyArtifactFileError', () => {
  const rawEnoent = "ENOENT: no such file or directory, stat '/Users/w/plot.png'";

  it('maps ENOENT to a moved-or-deleted message with the structured code', () => {
    const friendly = friendlyArtifactFileError('ENOENT', rawEnoent);
    expect(friendly.message).toBe(
      "This file was moved, renamed, or deleted, so it can't be previewed anymore."
    );
    expect(friendly.code).toBe('ENOENT');
    expect(friendly.message).not.toContain('ENOENT');
  });

  it.each(['EACCES', 'EPERM'] as const)('maps %s to a permission message', (code) => {
    const friendly = friendlyArtifactFileError(code, `${code}: permission denied`);
    expect(friendly.message).toBe("Biorouter doesn't have permission to read this file.");
    expect(friendly.code).toBe(code);
  });

  it.each(['EISDIR', 'ENOTDIR'] as const)('maps %s to a not-a-file message', (code) => {
    const friendly = friendlyArtifactFileError(code, `${code}: illegal operation`);
    expect(friendly.message).toBe("This path isn't a previewable file.");
    expect(friendly.code).toBe(code);
  });

  it('falls back to the caller message for unknown codes, without a code', () => {
    const friendly = friendlyArtifactFileError('EMFILE', 'EMFILE: too many open files');
    expect(friendly).toEqual({ message: 'EMFILE: too many open files' });
  });

  it('keeps our own human-written errors (no code) untouched', () => {
    const message = "Access denied: path '/etc/passwd' is outside allowed directories";
    expect(friendlyArtifactFileError(undefined, message)).toEqual({ message });
  });
});
