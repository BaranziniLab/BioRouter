import { describe, expect, it } from 'vitest';
import {
  CLAMP_CHAR_THRESHOLD,
  CLAMP_LINE_THRESHOLD,
  describeMessageLength,
  formatBytes,
  formatMessageExtent,
  measureMessageExtent,
} from './messageClamp';

/** n lines of `text`, joined — not a trailing newline, which is a different case. */
const lines = (n: number, text = 'x') => Array.from({ length: n }, () => text).join('\n');

describe('the clamp threshold', () => {
  it('does not clamp at exactly ten lines, and does at eleven', () => {
    // The design says "clamp ABOVE the threshold … and never below it". Ten is
    // not above ten. This pair is the whole rule.
    expect(measureMessageExtent(lines(CLAMP_LINE_THRESHOLD)).shouldClamp).toBe(false);
    expect(measureMessageExtent(lines(CLAMP_LINE_THRESHOLD + 1)).shouldClamp).toBe(true);
  });

  it('does not clamp at exactly 600 characters, and does at 601', () => {
    expect(measureMessageExtent('a'.repeat(CLAMP_CHAR_THRESHOLD)).shouldClamp).toBe(false);
    expect(measureMessageExtent('a'.repeat(CLAMP_CHAR_THRESHOLD + 1)).shouldClamp).toBe(true);
  });

  it('clamps a long single-line message that has no line breaks at all', () => {
    // The character threshold exists for exactly this: one unbroken paragraph is
    // one line and can still be a wall of text.
    const wall = 'word '.repeat(200).trim();
    const extent = measureMessageExtent(wall);
    expect(extent.lines).toBe(1);
    expect(extent.shouldClamp).toBe(true);
  });

  it('does not clamp a short message, whatever its shape', () => {
    expect(measureMessageExtent('hello').shouldClamp).toBe(false);
    expect(measureMessageExtent('one\ntwo\nthree').shouldClamp).toBe(false);
    expect(measureMessageExtent('').shouldClamp).toBe(false);
  });

  it('does not clamp a nine-line message even with a trailing newline', () => {
    // "a\n" is one terminated line, not two, the second of which is empty.
    expect(measureMessageExtent(`${lines(9)}\n`).lines).toBe(9);
    expect(measureMessageExtent(`${lines(9)}\n`).shouldClamp).toBe(false);
  });
});

describe('counting', () => {
  it('counts an empty message as one empty line and no words', () => {
    const extent = measureMessageExtent('');
    expect(extent).toMatchObject({ lines: 1, words: 0, bytes: 0, chars: 0 });
  });

  it('counts UTF-8 bytes, not characters', () => {
    // A message of CJK is three times larger on the wire than `.length` says,
    // and "KB" is a claim about size.
    const extent = measureMessageExtent('日本語');
    expect(extent.chars).toBe(3);
    expect(extent.bytes).toBe(9);
  });

  it('collapses runs of whitespace when counting words', () => {
    expect(measureMessageExtent('  one   two \n three  ').words).toBe(3);
  });
});

describe('the count label', () => {
  it('states a line-structured message in lines and bytes', () => {
    // A traceback: many hard breaks, each line well under a terminal width.
    const traceback = Array.from(
      { length: 14 },
      (_, i) => `  File "run_pipeline.py", line ${i}, in main`
    ).join('\n');
    const extent = measureMessageExtent(traceback);
    expect(extent.shape).toBe('lines');
    expect(formatMessageExtent(extent)).toMatch(/^14 lines · \d+(\.\d)? (B|KB)$/);
  });

  it('states running prose in words', () => {
    const prose = 'word '.repeat(128).trim();
    const extent = measureMessageExtent(prose);
    expect(extent.shape).toBe('prose');
    expect(formatMessageExtent(extent)).toBe('128 words');
  });

  it('treats several long paragraphs as prose, not as lines', () => {
    // Hard breaks alone do not make a log — the lines have to be short too.
    const paragraphs = ['A'.repeat(300), 'B'.repeat(300), 'C'.repeat(300)].join('\n\n');
    expect(measureMessageExtent(paragraphs).shape).toBe('prose');
  });

  it('singularises', () => {
    expect(formatMessageExtent({ ...measureMessageExtent('hi'), shape: 'prose', words: 1 })).toBe(
      '1 word'
    );
    expect(
      formatMessageExtent({ ...measureMessageExtent('hi'), shape: 'lines', lines: 1, bytes: 5 })
    ).toBe('1 line · 5 B');
  });

  it('formats bytes the way a file manager does', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(842)).toBe('842 B');
    expect(formatBytes(1024)).toBe('1 KB');
    expect(formatBytes(8602)).toBe('8.4 KB');
    expect(formatBytes(86016)).toBe('84 KB');
    expect(formatBytes(1258291)).toBe('1.2 MB');
  });
});

describe('describeMessageLength', () => {
  it('returns the decision and the string the view needs, together', () => {
    const short = describeMessageLength('hello');
    expect(short.shouldClamp).toBe(false);

    const long = describeMessageLength(lines(214, 'a'.repeat(40)));
    expect(long.shouldClamp).toBe(true);
    expect(long.label).toMatch(/^214 lines · /);
  });
});
