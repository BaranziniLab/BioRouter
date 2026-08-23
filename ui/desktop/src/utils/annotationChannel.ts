/**
 * The preview panel → composer hand-off for annotations.
 *
 * A `CustomEvent` on `window`, scoped by session id, in the same shape as
 * `restore-chat-input` next door. The panel and the composer are siblings under
 * a chat surface with no shared state between them, and threading a callback
 * down would mean every mount site — including the two read-only transcript
 * surfaces that must NOT receive annotations — growing a prop it does not want.
 */

export const ANNOTATION_EVENT = 'artifact-annotation';

export type ArtifactAnnotation = {
  /** Which chat this belongs to. A panel in another window must not reach it. */
  sessionId: string;
  /** Absolute path of the cropped PNG, already written to the temp dir. */
  imagePath: string;
  /** What the crop was taken from, for the chip's label and the model's context. */
  sourceTitle: string;
  /** File path or URL of the artifact, when there is one. */
  sourceLocator?: string;
  /** Crop size in CSS pixels, for the chip's subtitle. */
  width: number;
  height: number;
};

export function sendArtifactAnnotation(annotation: ArtifactAnnotation): void {
  window.dispatchEvent(new CustomEvent(ANNOTATION_EVENT, { detail: annotation }));
}

export function onArtifactAnnotation(
  sessionId: string | null | undefined,
  handler: (annotation: ArtifactAnnotation) => void
): () => void {
  const listener = (event: Event) => {
    const detail = (event as CustomEvent<ArtifactAnnotation>).detail;
    if (!detail?.imagePath) return;
    // Scoped, not broadcast: several chats can be on screen at once, and an
    // annotation belongs to the one whose panel produced it.
    if ((detail.sessionId ?? null) !== (sessionId ?? null)) return;
    handler(detail);
  };
  window.addEventListener(ANNOTATION_EVENT, listener);
  return () => window.removeEventListener(ANNOTATION_EVENT, listener);
}

/**
 * The prose that travels with the crop.
 *
 * Sent as a short labelled block rather than a JSON blob glued onto the
 * message — the same shape VS Code uses for its element context, and markedly
 * easier for a model to read than the concatenated-`<div data-element=…>`
 * approach bolt.diy takes with the same information.
 *
 * Deliberately carries **no coordinates**. Anthropic's guidance is to crop for
 * fine targets rather than describe a box, and the arithmetic agrees: a small
 * crop costs a few hundred visual tokens against several thousand for a full
 * screenshot that then gets downscaled, moving the very coordinates you were
 * pointing with. Coordinate conventions are also provider-specific — Claude
 * wants absolute pixels and warns against normalized ones, Gemini emits
 * normalized — and a crop is the one payload that is universal.
 */
export function annotationContextText(annotation: ArtifactAnnotation): string {
  const source = annotation.sourceLocator
    ? `${annotation.sourceTitle} (${annotation.sourceLocator})`
    : annotation.sourceTitle;
  return [
    '[Selected region from the preview panel]',
    `Source: ${source}`,
    `Region: ${Math.round(annotation.width)}×${Math.round(annotation.height)} px`,
    'The attached image is the region the user selected.',
  ].join('\n');
}
