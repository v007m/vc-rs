import { vi } from 'vitest';

/** jsdom has no layout, so `bind:clientWidth` and its kin need a ResizeObserver that never fires. */
export function stubResizeObserver(): void {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    },
  );
}
