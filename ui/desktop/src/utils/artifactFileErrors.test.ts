import { describe, expect, it } from 'vitest';
import {
  artifactFileErrorMessage,
  friendlyArtifactFileError,
  NEVER_CREATED_MESSAGE,
} from './artifactFileErrors';

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

// The panel opens paths from two sources with very different standing: a
// successful tool call that WROTE the file, and assistant prose that merely
// named one. The generic ENOENT copy asserts the file once existed, which for a
// suggested path is false — the reproduced case was a spec file the assistant
// proposed writing, which had never been created.
describe('artifactFileErrorMessage', () => {
  const enoent = {
    error: "This file was moved, renamed, or deleted, so it can't be previewed anymore.",
    code: 'ENOENT',
  };

  it('says a mentioned-only path was never created', () => {
    expect(artifactFileErrorMessage(enoent, { mentionedOnly: true })).toBe(NEVER_CREATED_MESSAGE);
    expect(NEVER_CREATED_MESSAGE).not.toMatch(/moved|renamed|deleted/);
  });

  it('keeps the moved-or-deleted copy for a path something confirmed', () => {
    // A tool call wrote it, or the panel's existence gate found it on disk —
    // both clear the flag, so the file really did disappear afterwards.
    expect(artifactFileErrorMessage(enoent, { mentionedOnly: false })).toBe(enoent.error);
    expect(artifactFileErrorMessage(enoent)).toBe(enoent.error);
  });

  it('never overrides a non-ENOENT failure, whoever named the path', () => {
    // A permission error, a directory, or an allowlist refusal says something
    // true about the path regardless of provenance.
    for (const file of [
      { error: "Biorouter doesn't have permission to read this file.", code: 'EACCES' },
      { error: "This path isn't a previewable file.", code: 'EISDIR' },
      { error: "Access denied: path '/etc/passwd' is outside allowed directories" },
    ]) {
      expect(artifactFileErrorMessage(file, { mentionedOnly: true })).toBe(file.error);
    }
  });
});
