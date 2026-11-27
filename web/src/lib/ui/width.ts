import type { Attachment } from 'svelte/attachments';

/**
 * Reports an element's width whenever it changes. Without a ResizeObserver (jsdom) it never
 * reports, so the width stays at its unmeasured 0.
 */
export function watchWidth(set: (width: number) => void): Attachment<HTMLElement> {
  return (el) => {
    if (typeof ResizeObserver !== 'function') return;
    const sized = new ResizeObserver(() => set(el.clientWidth));
    sized.observe(el);
    return () => sized.disconnect();
  };
}
