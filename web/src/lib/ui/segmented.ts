/**
 * The segmented control's sliding lens (Strata 1.1, D615): one glass lens, the track's
 * ::before in vibranium.css, that springs between options. Attach it to a `.st-segmented`
 * track (`{@attach lens}`); it follows whichever button is `aria-pressed="true"`, and the track
 * shows no lens when none is. Before this runs (and without layout) the pressed button draws
 * its own lens, so nothing is lost.
 */
export function lens(track: HTMLElement): () => void {
  const place = (): void => {
    const on = track.querySelector<HTMLElement>('button[aria-pressed="true"]');
    if (!on) {
      track.dataset['lens'] = 'none';
      return;
    }
    track.style.setProperty('--lens-x', `${on.offsetLeft}px`);
    track.style.setProperty('--lens-w', `${on.offsetWidth}px`);
    track.dataset['lens'] = 'on';
  };
  place();
  const pressed = new MutationObserver(place);
  pressed.observe(track, { subtree: true, childList: true, attributeFilter: ['aria-pressed'] });
  const sized = typeof ResizeObserver === 'function' ? new ResizeObserver(place) : undefined;
  sized?.observe(track);
  return () => {
    pressed.disconnect();
    sized?.disconnect();
    delete track.dataset['lens'];
  };
}
