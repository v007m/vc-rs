/**
 * The style audit's system rules (D667–D669): the checks that keep one button surface, one
 * disclosure mark, capsule tags, Strata checkboxes and radios, single-row toolbars and inset table
 * hairlines across every route. Like `page.ts`, these run in the audited page: `pageScript`
 * sends their source with the rest, so the helpers they call are only declared here.
 */

type Rgba = [number, number, number, number];
type Box = { x: number; y: number; w: number; h: number };
declare function rgba(color: string): Rgba;
declare function boxOf(el: Element): Box;
declare function shown(el: Element): boolean;
declare function describe(el: Element): string;
declare function radiusOf(el: Element): number;
declare function paints(el: Element): { fill: boolean; border: boolean; lift: boolean };

/** A secondary button, and a ghost one while pressed or open: all draw the one surface. */
export const SECONDARY =
  '.st-btn:not(.st-btn--primary, .st-btn--danger, .st-btn--ghost), .st-btn--ghost:is([aria-pressed=true], [aria-expanded=true]), .cm-diagnosticAction';
/** Tags that are badges whatever their class: method and status-code tags. */
export const TAGS = '.st-badge, .method, span.code';
/** The glyphs a disclosure used to draw before the one chevron (D668). */
export const GLYPHS = ['▶', '▸', '►', '▾', '▼', '▿', '›', '⌄'];

/** `var(--vc-btn-surface)` as the page resolves it, through a probe element. */
function buttonSurface(): Rgba {
  const probe = document.createElement('span');
  probe.style.setProperty('background-color', 'var(--vc-btn-surface)');
  document.body.append(probe);
  const fill = rgba(getComputedStyle(probe).backgroundColor);
  probe.remove();
  return fill;
}

/** Every secondary button draws the same opaque fill on every route (D667). */
function buttonFills(): string[] {
  const want = buttonSurface();
  const same = (a: Rgba) => a.every((v, i) => Math.abs(v - (want[i] ?? 0)) <= (i === 3 ? 0.01 : 2));
  return [...document.querySelectorAll(SECONDARY)]
    .filter(shown)
    .filter((el) => !same(rgba(getComputedStyle(el).backgroundColor)))
    .map(
      (el) =>
        `button fill: ${describe(el)} is ${getComputedStyle(el).backgroundColor}, not the secondary surface`,
    );
}

/** A method or status-code tag that draws a box is a capsule like every badge (D668). */
function tagShapes(): string[] {
  const boxed = (el: Element) => paints(el).fill || paints(el).border;
  return [...document.querySelectorAll(TAGS)]
    .filter((el) => shown(el) && (el.matches('.st-badge') || boxed(el)))
    .filter((el) => boxOf(el).h <= 28 && radiusOf(el) < boxOf(el).h / 2 - 0.5)
    .map(
      (el) =>
        `badge: ${describe(el)} is ${boxOf(el).h}px tall with radius ${radiusOf(el).toFixed(1)}`,
    );
}

/** One disclosure mark: a rotating chevron, never a glyph or the native marker (D668). */
function disclosures(): string[] {
  const out: string[] = [];
  for (const el of document.querySelectorAll('body *')) {
    if (el.children.length > 0 || !shown(el)) continue;
    if (GLYPHS.includes((el.textContent ?? '').trim()))
      out.push(`disclosure: ${describe(el)} draws a glyph, not the chevron`);
  }
  for (const summary of document.querySelectorAll('summary')) {
    if (!shown(summary)) continue;
    const look = getComputedStyle(summary);
    if (look.display === 'list-item' && look.listStyleType !== 'none')
      out.push(`disclosure: ${describe(summary)} shows the native marker`);
    else if (!summary.querySelector('.vc-chev'))
      out.push(`disclosure: ${describe(summary)} has no chevron`);
  }
  return out;
}

/** A toolbar keeps one row; what does not fit moves into its overflow menu (D669). */
function toolbars(): string[] {
  return [...document.querySelectorAll('.vc-toolbar, [role=toolbar]')]
    .filter(shown)
    .flatMap((bar) => {
      const kids = [...bar.children].map(boxOf).filter((b) => b.w > 0 && b.h > 0);
      if (kids.length < 2) return [];
      const top = Math.min(...kids.map((b) => b.y));
      const tallest = Math.max(...kids.map((b) => b.h));
      const wrapped = kids.some((b) => b.y >= top + tallest - 1);
      return wrapped ? [`toolbar: ${describe(bar)} wraps onto a second row`] : [];
    });
}

/** A checkbox is Strata's, so it takes the space accent in both themes (D668). */
function checkboxes(): string[] {
  return [...document.querySelectorAll('input[type=checkbox]')]
    .filter(shown)
    .filter((el) => !el.matches('.st-check, .st-switch'))
    .map((el) => `checkbox: ${describe(el)} is the browser's own, not .st-check`);
}

/** Strata has no radio: every radio is the round .vc-radio, never the browser's own (D721). */
function radios(): string[] {
  return [...document.querySelectorAll('input[type=radio]')]
    .filter(shown)
    .filter((el) => !el.matches('.vc-radio') || getComputedStyle(el).appearance !== 'none')
    .map((el) => `radio: ${describe(el)} is the browser's own, not .vc-radio`);
}

/** A collapsed cell border runs edge to edge; table hairlines stop 12px short (D669). */
function hairlines(): string[] {
  const found = new Set<string>();
  for (const cell of document.querySelectorAll('th, td')) {
    if (!shown(cell)) continue;
    const s = getComputedStyle(cell);
    for (const side of ['top', 'bottom']) {
      const width = parseFloat(s.getPropertyValue(`border-${side}-width`));
      if (width > 0 && rgba(s.getPropertyValue(`border-${side}-color`))[3] > 0.02)
        found.add(`hairline: ${describe(cell.closest('table') ?? cell)} draws cell borders`);
    }
  }
  return [...found];
}

/** The system rules, in one pass. */
function system(): string[] {
  return [
    ...buttonFills(),
    ...tagShapes(),
    ...disclosures(),
    ...toolbars(),
    ...checkboxes(),
    ...radios(),
    ...hairlines(),
  ];
}

/** The functions above, in the order `pageScript` sends them. */
export const systemFns = [
  buttonSurface,
  buttonFills,
  tagShapes,
  disclosures,
  toolbars,
  checkboxes,
  radios,
  hairlines,
  system,
];
