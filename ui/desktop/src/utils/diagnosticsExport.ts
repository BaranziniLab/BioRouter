export const MAX_DIAGNOSTICS_ARCHIVE_BYTES = 128 * 1024 * 1024;

export type DiagnosticsArchivePayload = ArrayBuffer | ArrayBufferView;

export function diagnosticsArchiveFilename(sessionId: string): string {
  const safeSessionId = sessionId
    .trim()
    .replace(/[^a-zA-Z0-9._-]+/g, '_')
    .replace(/^[_.]+|[_.]+$/g, '')
    .slice(0, 120);

  return `diagnostics_${safeSessionId || 'session'}.zip`;
}

export function diagnosticsArchiveBytes(payload: DiagnosticsArchivePayload): Uint8Array {
  const bytes =
    payload instanceof ArrayBuffer
      ? new Uint8Array(payload)
      : new Uint8Array(payload.buffer, payload.byteOffset, payload.byteLength);

  if (bytes.byteLength < 4) {
    throw new Error('The diagnostics archive is empty.');
  }
  if (bytes.byteLength > MAX_DIAGNOSTICS_ARCHIVE_BYTES) {
    throw new Error('The diagnostics archive is too large to save.');
  }

  const hasZipSignature =
    bytes[0] === 0x50 &&
    bytes[1] === 0x4b &&
    ((bytes[2] === 0x03 && bytes[3] === 0x04) ||
      (bytes[2] === 0x05 && bytes[3] === 0x06) ||
      (bytes[2] === 0x07 && bytes[3] === 0x08));

  if (!hasZipSignature) {
    throw new Error('The diagnostics response is not a valid ZIP archive.');
  }

  return bytes;
}
