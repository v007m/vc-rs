/**
 * The style audit's in-page half. These functions run in the audited page, not in Node: the
 * spec sends their source as one init script (`pageScript`), so each may call the others by
 * name but none may use an import or a module-level value. Each check returns its findings
 * as strings naming the element, so a failure reads as a list of what to fix.
 */

import { GLYPHS, SECONDARY, systemFns, TAGS } from './system.ts';

/** The system rules (`system.ts`), defined in the same page script. */
declare function system(): string[];

type Rgba = [number, number, number, number];
type Box = { x: number; y: number; w: number; h: number };
/** A run of text, an input's value or placeholder, or an icon, for the contrast pass. */
export type Ink = { box: Box; ink: Rgba; min: number; what: string };

/** The sRGB value of any CSS colour, through a 1px canvas (computed styles keep oklch). */
function rgba(color: string): Rgba {
  const g = new OffscreenCanvas(1, 1).getContext('2d', { willReadFrequently: true });
  if (!g) return [0, 0, 0, 0];
  g.clearRect(0, 0, 1, 1);
  g.fillStyle = color;
  g.fillRect(0, 0, 1, 1);
  const [r = 0, gr = 0, b = 0, a = 0] = g.getImageData(0, 0, 1, 1).data;
  return [r, gr, b, a / 255];
}

function boxOf(el: Element): Box {
  const r = el.getBoundingClientRect();
  return { x: r.x, y: r.y, w: r.width, h: r.height };
}

/** The product of the element's and its ancestors' opacity. */
function opacityOf(el: Element): number {
  let o = 1;
  for (let at: Element | null = el; at; at = at.parentElement)
    o *= Number(getComputedStyle(at).opacity);
  return o;
}

/** On screen and not behind a modal: a modal dialog makes the rest of the page inert. */
function shown(el: Element): boolean {
  const b = boxOf(el);
  if (b.w < 1 || b.h < 1 || b.x + b.w <= 0 || b.y + b.h <= 0) return false;
  if (b.x >= innerWidth || b.y >= innerHeight) return false;
  if (el.closest('[inert], [aria-hidden="true"] .vc-sr, .vc-sr')) return false;
  const modal = [...document.querySelectorAll('dialog')].find((d) => d.matches(':modal'));
  if (modal && !modal.contains(el)) return false;
  if (getComputedStyle(el).visibility !== 'visible') return false;
  // under a scrim, a drawer or a menu: what is on top is audited, not what it covers
  const hit = document.elementFromPoint(
    Math.min(innerWidth - 1, Math.max(0, b.x + b.w / 2)),
    Math.min(innerHeight - 1, Math.max(0, b.y + b.h / 2)),
  );
  if (hit && hit !== el && !el.contains(hit) && !hit.contains(el)) return false;
  return opacityOf(el) > 0.05;
}

function describe(el: Element): string {
  const cls = [...el.classList]
    .filter((c) => !c.startsWith('s-'))
    .slice(0, 2)
    .join('.');
  const text = (el.getAttribute('aria-label') ?? el.textContent ?? '').trim().replace(/\s+/g, ' ');
  return `${el.tagName.toLowerCase()}${cls ? `.${cls}` : ''} "${text.slice(0, 32)}"`;
}

function px(value: string, of: number): number {
  return value.endsWith('%') ? (parseFloat(value) / 100) * of : parseFloat(value) || 0;
}

/** A split button's halves join flat; only the corners on the group's outside count. */
function outerCorners(el: Element, s: CSSStyleDeclaration): string[] {
  const left = [s.borderTopLeftRadius, s.borderBottomLeftRadius];
  const right = [s.borderTopRightRadius, s.borderBottomRightRadius];
  const group = el.closest('.btn-split');
  if (!group) return [...left, ...right];
  const b = boxOf(el);
  const g = boxOf(group);
  return [...(b.x - g.x < 1 ? left : []), ...(g.x + g.w - (b.x + b.w) < 1 ? right : [])];
}

/** The smallest of the outer corner radii as drawn (CSS clamps a radius to half the side). */
function radiusOf(el: Element): number {
  const b = boxOf(el);
  const corners = outerCorners(el, getComputedStyle(el));
  if (corners.length === 0) return b.h / 2;
  // a half of a split has one rounded side, so its corners may take the whole width
  const across = corners.length === 4 ? b.w / 2 : b.w;
  return Math.min(
    ...corners.map((c) => Math.min(px(c.split(' ')[0] ?? '0', b.w), b.h / 2, across)),
  );
}

/** What the element draws itself: a fill, a border, an outer (elevation) shadow. */
function paints(el: Element): { fill: boolean; border: boolean; lift: boolean } {
  const s = getComputedStyle(el);
  // gradients alone are line art (grid lines, a select's chevron), not a surface
  const fill =
    rgba(s.backgroundColor)[3] > 0.02 ||
    s.backgroundImage.includes('url(') ||
    s.backdropFilter !== 'none';
  const border = ['Top', 'Right', 'Bottom', 'Left'].some(
    (side) =>
      parseFloat(s.getPropertyValue(`border-${side.toLowerCase()}-width`)) > 0 &&
      rgba(s.getPropertyValue(`border-${side.toLowerCase()}-color`))[3] > 0.02,
  );
  const lift =
    s.boxShadow !== 'none' && s.boxShadow.split(/,(?![^(]*\))/).some((sh) => !sh.includes('inset'));
  return { fill, border, lift };
}

const CONTROL =
  'button, a.st-btn, a.st-icon-btn, input:not([type=checkbox], [type=radio], [type=range], [type=hidden]), select, .st-input, .st-segmented, [role=tab], [role=combobox]';
const ROW =
  '[role=menuitem], [role=menuitemradio], [role=menuitemcheckbox], [role=option], [role=treeitem], [role=row], [role=listitem], li, tr, .st-row, .st-menu-item, .vc-palette-item, .st-switch, .st-check, .cm-editor *';

/** A single-line control the shape rules cover (a segmented track, not its buttons). */
function controls(): Element[] {
  return [...document.querySelectorAll(CONTROL)].filter((el) => {
    if (!shown(el) || el.closest(ROW) || boxOf(el).h > 44) return false;
    if (el.parentElement?.closest('.st-segmented, .st-input')) return false;
    return !(el.matches('.st-input') && el.querySelector('textarea'));
  });
}

/** A control that shows its box: it paints one, or it is a Strata control (ghosts included). */
function drawn(el: Element): boolean {
  const p = paints(el);
  return p.fill || p.border || p.lift || el.matches('.st-btn, .st-icon-btn, .st-input, .vc-tab');
}

/** Controls side by side (overlapping rows, at most 16px apart) must share one height. */
function rowHeights(): string[] {
  // a bare text button (a section header's toggle) draws no box, so it has no height to match
  const all = controls().filter((el) => !el.matches('textarea') && drawn(el));
  const found = new Set<string>();
  for (const a of all) {
    for (const b of all) {
      if (a === b || a.contains(b) || b.contains(a)) continue;
      const p = boxOf(a);
      const q = boxOf(b);
      const overlap = Math.min(p.y + p.h, q.y + q.h) - Math.max(p.y, q.y);
      const gap = Math.max(q.x - (p.x + p.w), p.x - (q.x + q.w));
      if (overlap < Math.min(p.h, q.h) / 2 || gap > 16 || gap < -1) continue;
      if (Math.abs(p.h - q.h) > 0.5 && p.x < q.x)
        found.add(`row: ${describe(a)} is ${p.h}px beside ${describe(b)} at ${q.h}px`);
    }
  }
  return [...found];
}

/** Every single-line control and badge is a capsule: radius = height / 2. */
function capsules(): string[] {
  const badges = [...document.querySelectorAll('.st-badge, .st-segmented button, .vc-tab')].filter(
    shown,
  );
  return [...controls().filter(drawn), ...badges]
    .filter((el) => !el.matches('textarea, .st-input--rect'))
    .filter((el) => radiusOf(el) < boxOf(el).h / 2 - 0.5)
    .map(
      (el) =>
        `capsule: ${describe(el)} is ${boxOf(el).h}px tall with radius ${radiusOf(el).toFixed(1)}`,
    );
}

function surface(el: Element): boolean {
  const p = paints(el);
  return (p.fill || p.border || p.lift) && radiusOf(el) >= 0 && !el.matches(CONTROL);
}

/** The nearest ancestor that draws a rounded surface, and the gaps to its four corners. */
function outerOf(el: Element): { outer: Element; d: number[] } | undefined {
  for (let at = el.parentElement; at && at !== document.body; at = at.parentElement) {
    if (!surface(at) || radiusOf(at) < 2) continue;
    const o = boxOf(at);
    const c = boxOf(el);
    const dx = [c.x - o.x, o.x + o.w - (c.x + c.w)];
    const dy = [c.y - o.y, o.y + o.h - (c.y + c.h)];
    return { outer: at, d: [dx[0] ?? 0, dx[1] ?? 0, dy[0] ?? 0, dy[1] ?? 0] };
  }
  return undefined;
}

/** A surface in an outer one's corner (or a list row spanning it) takes outer − gap (±1px). */
function nested(): string[] {
  const out: string[] = [];
  for (const el of document.querySelectorAll('body *')) {
    if (!surface(el) || !shown(el) || el.closest('svg, .cm-editor')) continue;
    const b = boxOf(el);
    if (b.h <= 44 && !el.closest(ROW)) continue;
    const at = outerOf(el);
    if (!at) continue;
    const R = radiusOf(at.outer);
    const clips = getComputedStyle(at.outer).overflow !== 'visible';
    // in a clipping outer, an edge the element runs past (scrolled under it) is not a corner
    const edge = (d: number) => (clips && d < -0.5 ? Infinity : d);
    const [l, r, t, bt] = at.d.map(edge) as [number, number, number, number];
    const corner = Math.min(Math.max(l, t), Math.max(r, t), Math.max(l, bt), Math.max(r, bt));
    const row = el.matches(ROW) && Math.max(l, r) < R ? Math.max(l, r) : Infinity;
    const gap = Math.min(corner, row);
    if (gap >= R - 0.5 || gap < -0.5) continue;
    if (clips && gap < 1) continue;
    const want = R - gap;
    const has = radiusOf(el);
    if (Math.abs(has - want) > 1)
      out.push(
        `radius: ${describe(el)} is ${has.toFixed(1)}, ${gap.toFixed(0)}px inside ${describe(at.outer)} (${R}); want ${want.toFixed(0)}`,
      );
  }
  return out;
}

/** Dialogs centre in the window, the palette lands centred in the top third, and menus sit
 * at their trigger without leaving the window. */
function overlays(): string[] {
  const out: string[] = [];
  for (const d of document.querySelectorAll('dialog[open]')) {
    const b = boxOf(d);
    const dx = b.x + b.w / 2 - innerWidth / 2;
    if (d.matches('.vc-sheet')) continue;
    if (Math.abs(dx) > 2) out.push(`centre: ${describe(d)} is ${dx.toFixed(0)}px off centre`);
    if (d.matches('.palette')) {
      if (b.y < innerHeight * 0.08 || b.y > innerHeight / 3)
        out.push(`centre: the palette's top is at ${b.y}px`);
    } else if (Math.abs(b.y + b.h / 2 - innerHeight / 2) > 2) {
      out.push(
        `centre: ${describe(d)} is ${(b.y + b.h / 2 - innerHeight / 2).toFixed(0)}px off the middle`,
      );
    }
  }
  for (const p of document.querySelectorAll('[popover]:popover-open')) {
    const b = boxOf(p);
    if (b.x < 0 || b.y < 0 || b.x + b.w > innerWidth || b.y + b.h > innerHeight)
      out.push(`clipped: ${describe(p)} leaves the window`);
    const trigger = document.querySelector(`[popovertarget="${p.id}"], [aria-controls="${p.id}"]`);
    if (!trigger) continue;
    const t = boxOf(trigger);
    const gapY = Math.max(b.y - (t.y + t.h), t.y - (b.y + b.h));
    const gapX = Math.max(b.x - (t.x + t.w), t.x - (b.x + b.w));
    if (gapY > 12 || gapX > 12)
      out.push(`anchor: ${describe(p)} is ${Math.max(gapX, gapY)}px from its trigger`);
  }
  return out;
}

const OVERLAY = 'dialog, [popover], .st-overlay';
const WELL = '.vc-well, .vc-wash, pre, code, .cm-editor, .cm-editor *, svg *';
const MARKED =
  '[aria-selected="true"], [aria-current], [aria-checked="true"], [aria-pressed="true"]';

/** Nothing inside a card draws a box: no card on a card, and empty states float on the card. */
function cardOnCard(): string[] {
  const out: string[] = [];
  const cards = '.st-card, dialog.st-overlay, [popover].st-overlay';
  for (const el of document.querySelectorAll('body *')) {
    if (!el.parentElement?.closest(cards) || !shown(el) || el.matches(OVERLAY)) continue;
    const p = paints(el);
    if (el.matches('.vc-empty') && (p.fill || p.border || p.lift)) {
      out.push(`card on card: the empty state ${describe(el)} draws a box`);
      continue;
    }
    const b = boxOf(el);
    if (b.h <= 40 || b.w <= 40 || el.matches(CONTROL) || el.closest(`${MARKED}, ${WELL}`)) continue;
    if (el.matches('.st-card') || p.lift || (p.fill && (p.border || !el.matches(WELL))))
      out.push(
        `card on card: ${describe(el)} draws a box inside ${describe(el.parentElement.closest(cards) ?? el)}`,
      );
  }
  return out;
}

/** The structural checks, in one pass. */
function structure(): string[] {
  return [...rowHeights(), ...capsules(), ...nested(), ...overlays(), ...cardOnCard(), ...system()];
}

/** The page's text runs, field text and icons, with the ink each is drawn in. Icons get a
 * data-vc-audit-icon mark so the backdrop shot can hide them along with the text. */
function inks(): Ink[] {
  const out: Ink[] = [];
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const el = n.parentElement;
    if (!el || !n.textContent?.trim() || !shown(el) || el.closest('svg [aria-hidden], .vc-sr'))
      continue;
    const range = document.createRange();
    range.selectNodeContents(n);
    const disabled = el.closest(':disabled, [aria-disabled="true"]') !== null;
    const [r, g, b, a] = rgba(getComputedStyle(el).color);
    for (const rect of range.getClientRects()) {
      if (rect.width < 2 || rect.height < 4) continue;
      const box = { x: rect.x, y: rect.y, w: rect.width, h: rect.height };
      out.push({
        box,
        ink: [r, g, b, a * opacityOf(el)],
        min: disabled ? 3 : 4.5,
        what: describe(el),
      });
    }
  }
  for (const f of document.querySelectorAll(
    'input:not([type=checkbox], [type=radio], [type=range], [type=hidden]), select, textarea',
  )) {
    if (!shown(f)) continue;
    const el = f as HTMLInputElement;
    const empty = !el.value && el.placeholder;
    if (!el.value && !empty) continue;
    const s = getComputedStyle(el, empty ? '::placeholder' : null);
    const b = boxOf(el);
    const padL = parseFloat(getComputedStyle(el).paddingLeft);
    const box = { x: b.x + padL, y: b.y + b.h / 2 - 6, w: Math.min(b.w - padL * 2, 120), h: 12 };
    const [r, g, bl, a] = rgba(s.color);
    out.push({
      box,
      ink: [r, g, bl, a * opacityOf(el)],
      min: el.disabled ? 3 : 4.5,
      what: `${describe(el)} ${empty ? 'placeholder' : 'value'}`,
    });
  }
  for (const svg of document.querySelectorAll('svg')) {
    const b = boxOf(svg);
    // a logotype is exempt from WCAG contrast (1.4.3, 1.4.11), wherever it sits
    if (b.w > 28 || b.h > 28 || !shown(svg) || svg.closest('.cm-editor, .rail-brand')) continue;
    const shape = svg.querySelector('path, circle, rect, line, polyline, polygon');
    if (!shape) continue;
    const s = getComputedStyle(shape);
    const paint = s.stroke !== 'none' ? s.stroke : s.fill;
    if (paint === 'none' || paint.startsWith('url')) continue;
    svg.setAttribute('data-vc-audit-icon', '');
    const [r, g, bl, a] = rgba(paint);
    const disabled = svg.closest(':disabled, [aria-disabled="true"]') !== null;
    out.push({
      box: b,
      ink: [r, g, bl, a * opacityOf(svg)],
      min: disabled ? 2 : 3,
      what: `icon in ${describe(svg.parentElement ?? svg)}`,
    });
  }
  return out;
}

const FIELD = 'input:not([type=checkbox], [type=radio], [type=range]), select, textarea';

/** The rings (box-shadow spreads without offset or blur) the element draws outside itself. */
function rings(el: Element): number[] {
  const s = getComputedStyle(el).boxShadow;
  if (s === 'none') return [];
  return s
    .split(/,(?![^(]*\))/)
    .filter((sh) => !sh.includes('inset'))
    .map((sh) =>
      sh
        .replace(/(rgba?|color|oklch|oklab)\([^)]*\)/g, '')
        .trim()
        .split(/\s+/)
        .map(parseFloat),
    )
    .filter(
      ([x = 0, y = 0, blur = 0, spread = 0]) => x === 0 && y === 0 && blur === 0 && spread > 0,
    )
    .map(([, , , spread = 0]) => spread);
}

/** One focused element against the focus spec: a field's ring is 1.5px on the field (or its
 * .st-input frame) with no outline; anything else draws a 2px outline at most 2px away. */
function ringIssue(el: Element): string | undefined {
  if (el.matches(FIELD)) {
    const frame = el.closest('.st-input') ?? el;
    const outline = [frame, el].find(
      (e) =>
        parseFloat(getComputedStyle(e).outlineWidth) > 0 &&
        getComputedStyle(e).outlineStyle !== 'none',
    );
    if (outline) return `focus: ${describe(el)} draws an outline; a field's ring is a 1.5px shadow`;
    const spread = rings(frame);
    if (!spread.length) return `focus: ${describe(el)} shows no ring`;
    if (Math.max(...spread) > 1.51)
      return `focus: ${describe(el)} has a ${Math.max(...spread)}px ring; the spec is 1.5px`;
    if (frame !== el && rings(el).length)
      return `focus: ${describe(el)} draws a ring inside its frame`;
    return undefined;
  }
  const s = getComputedStyle(el);
  const width = s.outlineStyle === 'none' ? 0 : parseFloat(s.outlineWidth);
  if (width === 0 && !rings(el).length) return `focus: ${describe(el)} shows no ring`;
  if (width > 2.01 || parseFloat(s.outlineOffset) > 2.01 || Math.max(0, ...rings(el)) > 2.01)
    return `focus: ${describe(el)} has a ${width}px ring at ${s.outlineOffset}; the spec is 2px at 2px`;
  return undefined;
}

/** Focuses each control on screen (the spec pressed a key first, so focus is visible). */
function focusRings(): string[] {
  const out = new Set<string>();
  const all = [...document.querySelectorAll(`${CONTROL}, a[href], [role=switch], [tabindex="0"]`)]
    .filter((el) => shown(el) && !el.closest('.cm-editor') && !el.matches(':disabled'))
    .slice(0, 80);
  for (const el of all) {
    (el as HTMLElement).focus({ preventScroll: true });
    if (document.activeElement !== el || !el.matches(':focus-visible')) continue;
    const issue = ringIssue(el);
    if (issue) out.add(issue);
  }
  (document.activeElement as HTMLElement | null)?.blur();
  return [...out];
}

/** Every function above, as one script that leaves them on `window.vcAudit`. */
export function pageScript(): string {
  const fns = [
    rgba,
    boxOf,
    opacityOf,
    shown,
    describe,
    px,
    outerCorners,
    radiusOf,
    paints,
    controls,
    drawn,
    rowHeights,
    capsules,
    surface,
    outerOf,
    nested,
    overlays,
    cardOnCard,
    structure,
    inks,
    rings,
    ringIssue,
    focusRings,
    ...systemFns,
  ];
  const consts = { CONTROL, ROW, OVERLAY, WELL, MARKED, FIELD, SECONDARY, TAGS, GLYPHS };
  const values = Object.entries(consts).map(([k, v]) => `const ${k} = ${JSON.stringify(v)};`);
  return `(() => { ${values.join('\n')}\n${fns.map(String).join('\n')}\nwindow.vcAudit = { structure, inks, focusRings }; })();`;
}

declare global {
  interface Window {
    vcAudit: { structure: () => string[]; inks: () => Ink[]; focusRings: () => string[] };
  }
}
