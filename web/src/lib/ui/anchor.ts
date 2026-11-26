/**
 * Overlays grow out of what opened them (docs/design/ui.md §2, Strata 1.1). The frame starts
 * moved onto its anchor and clipped to the anchor's size, then springs to its own place and
 * size; the content stays full size the whole time (a clip and a translate, never a scale).
 * The clip morphs evenly: its edges and all four corners move together, from the anchor's
 * capsule radius to the frame's own, so every frame is one concentric shape. Once it lands the
 * clip is dropped (`is-rest`), so the frame's shadow shows; it comes back for the close.
 * Closing runs the same path back on the quicker spring after the content fades, and a reopen
 * mid-close reverses from wherever the frame is. The values reach CSS as custom properties
 * (CSSOM, which vc ui's CSP allows); `.vc-anchored` in vibranium.css does the motion.
 */

/** A rectangle in viewport pixels, as `getBoundingClientRect` gives it. */
export interface Box {
  readonly left: number;
  readonly top: number;
  readonly width: number;
  readonly height: number;
}

/** Where a frame starts: its offset onto the anchor and the clip that shows only the anchor's size. */
export interface Start {
  readonly x: string;
  readonly y: string;
  readonly clip: string;
}

/** The start for `frame` growing out of `anchor`: its top-left corner over the anchor's. */
export function startFrom(frame: Box, anchor: Box): Start {
  const right = Math.max(frame.width - anchor.width, 0);
  const bottom = Math.max(frame.height - anchor.height, 0);
  const radius = Math.min(anchor.height, anchor.width) / 2;
  return {
    x: `${Math.round(anchor.left - frame.left)}px`,
    y: `${Math.round(anchor.top - frame.top)}px`,
    clip: `inset(0px ${Math.round(right)}px ${Math.round(bottom)}px 0px round ${radius}px)`,
  };
}

const rest = new WeakMap<HTMLElement, Box>();

function place(frame: HTMLElement, anchor: Element, box: Box): void {
  const start = startFrom(box, anchor.getBoundingClientRect());
  const radius = getComputedStyle(frame).borderTopLeftRadius || '0px';
  frame.style.setProperty('--vc-from-x', start.x);
  frame.style.setProperty('--vc-from-y', start.y);
  frame.style.setProperty('--vc-from-clip', start.clip);
  frame.style.setProperty('--vc-to-clip', `inset(0px round ${radius})`);
}

function running(frame: HTMLElement): Animation[] {
  return typeof frame.getAnimations === 'function' ? frame.getAnimations() : [];
}

function clipMotion(frame: HTMLElement): Animation[] {
  return running(frame).filter(
    (animation) => (animation as Partial<CSSTransition>).transitionProperty === 'clip-path',
  );
}

/** Drops the clip once the frame has landed open, so nothing cuts its shadow. */
function settle(frame: HTMLElement): void {
  const landed = (): void => {
    if (!frame.classList.contains('is-closing')) frame.classList.add('is-rest');
  };
  const clip = clipMotion(frame);
  if (clip.length === 0) landed();
  else Promise.all(clip.map((animation) => animation.finished)).then(landed, () => undefined);
}

/** Puts the clip back at the frame's own shape, without motion, before it moves again. */
function unsettle(frame: HTMLElement): void {
  if (!frame.classList.contains('is-rest')) return;
  frame.classList.add('is-snap');
  frame.classList.remove('is-rest');
  void frame.getBoundingClientRect();
  frame.classList.remove('is-snap');
}

/** An anchor worth growing from: shown, and not the page itself (focus left on the body). */
function usable(anchor: Element | null | undefined): anchor is Element {
  if (!anchor || anchor === anchor.ownerDocument.body) return false;
  const box = anchor.getBoundingClientRect();
  return box.width > 0 && box.height > 0;
}

/**
 * Springs `frame` (a `.vc-anchored` element, already shown) out of `anchor`. Called while the
 * frame is still shrinking, it reverses the close from where the frame is instead. With no
 * usable anchor the frame only fades in.
 */
export function growFrom(frame: HTMLElement, anchor: Element | null | undefined): void {
  const closing = frame.classList.contains('is-closing');
  frame.classList.remove('is-closing', 'is-from', 'is-rest');
  if (closing && running(frame).length > 0) return settle(frame);
  if (!usable(anchor)) return settle(frame);
  const box = frame.getBoundingClientRect();
  rest.set(frame, box);
  place(frame, anchor, box);
  frame.classList.add('is-snap', 'is-from');
  void frame.getBoundingClientRect();
  frame.classList.remove('is-snap', 'is-from');
  settle(frame);
}

/**
 * Springs `frame` back into `anchor`, then calls `done` (the caller hides it there). A
 * `growFrom` before it lands reopens the frame and `done` is not called. With no motion
 * (reduced motion, or no layout) `done` runs at once, in the same task.
 */
export function shrinkInto(
  frame: HTMLElement,
  anchor: Element | null | undefined,
  done: () => void,
): void {
  if (usable(anchor)) {
    const moving = running(frame).length > 0;
    place(frame, anchor, (moving && rest.get(frame)) || frame.getBoundingClientRect());
  }
  unsettle(frame);
  frame.classList.add('is-closing', 'is-from');
  const clip = clipMotion(frame);
  if (clip.length === 0) {
    done();
    return;
  }
  Promise.all(clip.map((animation) => animation.finished)).then(
    () => {
      if (frame.classList.contains('is-closing')) done();
    },
    () => undefined,
  );
}
