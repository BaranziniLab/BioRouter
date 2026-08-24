import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  annotationContextText,
  onArtifactAnnotation,
  sendArtifactAnnotation,
  type ArtifactAnnotation,
} from './annotationChannel';

const annotation = (overrides: Partial<ArtifactAnnotation> = {}): ArtifactAnnotation => ({
  sessionId: 's1',
  imagePath: '/tmp/capture-annotation-abc.png',
  sourceTitle: 'Figure 2',
  sourceLocator: '/w/km-curve.html',
  width: 640,
  height: 420,
  ...overrides,
});

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()?.();
});

function listen(sessionId: string | null, handler: (a: ArtifactAnnotation) => void) {
  disposers.push(onArtifactAnnotation(sessionId, handler));
}

describe('routing an annotation to the right chat', () => {
  it('delivers to the session that produced it', () => {
    const seen = vi.fn();
    listen('s1', seen);
    sendArtifactAnnotation(annotation());
    expect(seen).toHaveBeenCalledOnce();
  });

  // Several chats can be on screen at once. A broadcast would attach one
  // panel's region to whichever composers happened to be mounted.
  it('does not deliver to a different chat', () => {
    const mine = vi.fn();
    const theirs = vi.fn();
    listen('s1', mine);
    listen('s2', theirs);
    sendArtifactAnnotation(annotation({ sessionId: 's1' }));
    expect(mine).toHaveBeenCalledOnce();
    expect(theirs).not.toHaveBeenCalled();
  });

  it('ignores a payload with no image', () => {
    const seen = vi.fn();
    listen('s1', seen);
    window.dispatchEvent(
      new CustomEvent('artifact-annotation', { detail: { sessionId: 's1' } })
    );
    expect(seen).not.toHaveBeenCalled();
  });

  it('stops listening once disposed', () => {
    const seen = vi.fn();
    const dispose = onArtifactAnnotation('s1', seen);
    dispose();
    sendArtifactAnnotation(annotation());
    expect(seen).not.toHaveBeenCalled();
  });
});

describe('what travels with the crop', () => {
  it('names the source and the region size', () => {
    const text = annotationContextText(annotation());
    expect(text).toContain('Figure 2');
    expect(text).toContain('/w/km-curve.html');
    expect(text).toContain('640×420');
  });

  it('reads as labelled prose, not a JSON blob', () => {
    // The shape VS Code uses for its element context, and markedly easier for
    // a model to read than the concatenated `<div data-element=…>` that
    // bolt.diy produces from the same information.
    const text = annotationContextText(annotation());
    expect(text).not.toContain('{');
    expect(text.split('\n').length).toBeGreaterThan(2);
  });

  // Deliberate, and the single most consequential decision in this payload.
  // Anthropic's guidance is to crop for fine targets rather than describe a
  // box; a crop is also the one payload that is provider-neutral, since Claude
  // wants absolute pixel coordinates and warns against normalized ones while
  // Gemini emits normalized.
  it('sends no coordinates at all', () => {
    const text = annotationContextText(annotation());
    expect(text).not.toMatch(/\bx\s*[:=]|\by\s*[:=]|left|top|coordinate/i);
  });

  it('copes with a source that has no locator', () => {
    const text = annotationContextText(annotation({ sourceLocator: undefined }));
    expect(text).toContain('Figure 2');
    expect(text).not.toContain('undefined');
  });
});

describe('how the panel and composer are wired', () => {
  const src = (relative: string) => readFileSync(join(__dirname, '..', relative), 'utf8');

  it('captures through the main process, not a DOM screenshot library', () => {
    const viewer = src('components/artifacts/ArtifactViewer.tsx');
    // `capturePage` is a compositor grab and is the only thing that can see
    // into the sandboxed `srcdoc` frames most artifacts render in. Every
    // DOM-walking alternative returns an empty box for a figure.
    expect(viewer).toContain('window.electron?.captureRegion');
    for (const library of ['html2canvas', 'html-to-image', 'modern-screenshot', 'snapdom']) {
      expect(viewer).not.toContain(library);
    }
  });

  it('offers annotation on every preview kind, not just one', () => {
    const viewer = src('components/artifacts/ArtifactViewer.tsx');
    const button = viewer.slice(viewer.indexOf('artifact-annotate') - 400);
    // Guarded only by `sessionId` (chat-vs-transcript), never by artifact kind.
    expect(button).not.toMatch(/artifact-annotate[\s\S]{0,200}activeArtifact\.kind ===/);
  });

  it('reuses the staged-attachment path rather than a second one', () => {
    const composer = src('components/ChatInput.tsx');
    expect(composer).toContain('onArtifactAnnotation');
    // Joining `pastedImages` is what inherits the thumbnail strip, the
    // hover-remove, the per-message cap and the path→base64 conversion.
    const listener = composer.slice(composer.indexOf('onArtifactAnnotation(sessionId'));
    expect(listener.slice(0, 1500)).toContain('setPastedImages');
    expect(listener.slice(0, 2500)).toContain('MAX_IMAGES_PER_MESSAGE');
  });
});
