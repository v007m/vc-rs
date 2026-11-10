import { describe, expect, it } from 'vitest';
import { applyEdits, indexAt, indexOfByte, lineDiff } from './text.ts';

describe('positions', () => {
  it('finds 1-based lines and character columns', () => {
    const text = 'a: 1\nnamé: ✓ok\n';
    expect(indexAt(text, 1, 1)).toBe(0);
    expect(indexAt(text, 2, 7)).toBe(text.indexOf('✓'));
    expect(indexAt(text, 9, 1)).toBe(text.length);
  });

  it('turns UTF-8 byte offsets into string indices', () => {
    expect(indexOfByte('é😀x', 2)).toBe(1);
    expect(indexOfByte('é😀x', 6)).toBe(3);
  });

  it('applies byte edits from the end, so earlier offsets stay valid', () => {
    const text = 'name: é\nid: x\n';
    const start = new TextEncoder().encode('name: é\nid: ').length;
    expect(
      applyEdits(text, [
        { start: 0, end: 4, newText: 'title' },
        { start, end: start + 1, newText: 'getsUser' },
      ]),
    ).toBe('title: é\nid: getsUser\n');
  });
});

describe('lineDiff', () => {
  it('keeps common lines and marks the rest', () => {
    const diff = lineDiff('a\nb\nc', 'a\nc\nd');
    expect(diff.map((l) => `${l.kind[0]}${l.text}`)).toEqual(['sa', 'db', 'sc', 'ad']);
    expect(diff.at(-1)).toMatchObject({ newNo: 3 });
  });

  it('is all same for equal text', () => {
    expect(lineDiff('x\ny', 'x\ny').every((l) => l.kind === 'same')).toBe(true);
  });
});
