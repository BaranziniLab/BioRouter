import { createHmac, randomBytes } from 'node:crypto';

const revisionKey = randomBytes(32);

/** Identity for the exact local bytes rendered in the preview panel. */
export function artifactSourceRevision(size: number, mtimeMs: number, content: Uint8Array): string {
  // Key the content identity so sending a revision to a provider does not hand
  // it a reusable dictionary-attack hash of the user's local file.
  const digest = createHmac('sha256', revisionKey).update(content).digest('hex');
  return `${size}:${mtimeMs}:${digest}`;
}
