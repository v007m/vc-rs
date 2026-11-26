import { describe, expect, it, vi } from 'vitest';
import { growFrom, shrinkInto, startFrom } from './anchor.ts';

const box = (left: number, top: number, width: number, height: number) => ({
  left,
  top,
  width,
  height,
});

function shown(el: HTMLElement, rect: ReturnType<typeof box>): HTMLElement {
  el.getBoundingClientRect = () => ({ ...rect, x: rect.left, y: rect.top }) as DOMRect;
  document.body.append(el);
  return el;
}

describe('startFrom', () => {
  it('moves the frame onto the anchor and clips it to the anchor, round as a capsule', () => {
    const start = startFrom(box(400, 120, 620, 360), box(1100, 12, 240, 32));
    expect(start).toEqual({
      x: '700px',
      y: '-108px',
      clip: 'inset(0px 380px 328px 0px round 16px)',
    });
  });

  it('never clips past the frame when the anchor is the bigger one', () => {
    expect(startFrom(box(0, 0, 100, 20), box(0, 0, 200, 40)).clip).toBe(
      'inset(0px 0px 0px 0px round 20px)',
    );
  });
});

describe('growFrom and shrinkInto', () => {
  it('sets the start as custom properties and leaves the frame at rest', () => {
    const frame = shown(document.createElement('div'), box(400, 120, 620, 360));
    const anchor = shown(document.createElement('button'), box(1100, 12, 240, 32));
    growFrom(frame, anchor);
    expect(frame.style.getPropertyValue('--vc-from-x')).toBe('700px');
    expect(frame.style.getPropertyValue('--vc-from-clip')).toContain('round 16px');
    expect([...frame.classList]).toEqual([]);
  });

  it('only fades when there is no anchor to grow from', () => {
    const frame = shown(document.createElement('div'), box(0, 0, 100, 100));
    growFrom(frame, document.body);
    growFrom(frame, null);
    expect(frame.style.getPropertyValue('--vc-from-x')).toBe('');
  });

  it('closes at once without motion, and a reopen clears the closing state', () => {
    const frame = shown(document.createElement('div'), box(0, 0, 100, 100));
    const anchor = shown(document.createElement('button'), box(10, 10, 40, 20));
    const done = vi.fn();
    shrinkInto(frame, anchor, done);
    expect(done).toHaveBeenCalledOnce();
    expect(frame.classList.contains('is-closing')).toBe(true);
    growFrom(frame, anchor);
    expect(frame.classList.contains('is-closing')).toBe(false);
  });
});
