/**
 * Symbol motion (Strata 1.1, D619): SVG stand-ins for SF Symbols' effects. `play` runs one
 * effect on an element (its keyframes are in vibranium.css), restarting it when it is already
 * running; under reduced motion the CSS shows nothing but the end state.
 */
export type SymbolEffect = 'rotate' | 'bounce' | 'wiggle';

/** Runs `effect` once on `element`: refresh rotates, confirm bounces, attention wiggles. */
export function play(element: Element | null | undefined, effect: SymbolEffect): void {
  if (!element) return;
  const name = `vc-sym-${effect}`;
  element.classList.remove(name);
  void element.getBoundingClientRect();
  element.classList.add(name);
  element.addEventListener('animationend', () => element.classList.remove(name), { once: true });
}
