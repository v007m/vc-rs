import { expect, test, type Page } from '@playwright/test';
import { lowContrast, weakEdges } from './contrast.ts';
import { routeEngine } from './engine.ts';
import { pageScript } from './page.ts';
import { states, type State } from './states.ts';

/**
 * The style audit (ui.md §2): every route and overlay, in both themes and at two window
 * sizes, against the rules that review kept finding by eye. Text under 4.5:1 (disabled 3:1,
 * icons 3:1) against the real backdrop, controls of one row at different heights, a
 * single-line control that is not a capsule, a nested surface off the concentric radius, an
 * overlay off centre or away from its trigger, a focus ring past its spec, and a box drawn
 * on a card, an unchecked control whose edge is under 3:1. Run it with `pnpm audit:style`.
 */

const HIDE_INK = `*, *::before, *::after, ::placeholder {
  color: transparent !important; -webkit-text-fill-color: transparent !important;
  text-shadow: none !important; caret-color: transparent !important; transition: none !important;
}
[data-vc-audit-icon] { visibility: hidden !important; }`;

/** Holds a stylesheet on the page through the CSSOM, which the CSP allows. */
async function withSheet<T>(page: Page, css: string, work: () => Promise<T>): Promise<T> {
  await page.evaluate((text) => {
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(text);
    document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
  }, css);
  try {
    return await work();
  } finally {
    await page.evaluate(() => {
      document.adoptedStyleSheets = document.adoptedStyleSheets.slice(0, -1);
    });
  }
}

async function open(page: Page, state: State): Promise<void> {
  await page.goto(state.path);
  await page.waitForTimeout(600);
  await state.act?.(page);
  await page.waitForTimeout(400);
  await page.evaluate(() => document.fonts.ready);
}

async function audit(page: Page, blank: Page): Promise<string[]> {
  const structure = await page.evaluate(() => window.vcAudit.structure());
  const inks = await page.evaluate(() => window.vcAudit.inks());
  const edges = await page.evaluate(() => window.vcAudit.edges());
  const shot = await withSheet(page, HIDE_INK, () => page.screenshot({ animations: 'disabled' }));
  const contrast = [
    ...(await lowContrast(blank, shot, inks)),
    ...(await weakEdges(blank, shot, edges)),
  ];
  await page.keyboard.press('Shift');
  const focus = await page.evaluate(() => window.vcAudit.focusRings());
  return [...structure, ...contrast, ...focus];
}

const sizes = [
  { width: 1440, height: 900 },
  { width: 1024, height: 768 },
] as const;

for (const scheme of ['light', 'dark'] as const) {
  for (const size of sizes) {
    test.describe(`${scheme} ${size.width}x${size.height}`, () => {
      test.use({ colorScheme: scheme, viewport: size, reducedMotion: 'reduce' });
      for (const state of states) {
        test(state.name, async ({ page, context }) => {
          // headless Chromium reports reduced transparency; audit the real glass
          const cdp = await context.newCDPSession(page);
          await cdp.send('Emulation.setEmulatedMedia', {
            features: [
              { name: 'prefers-color-scheme', value: scheme },
              { name: 'prefers-reduced-motion', value: 'reduce' },
              { name: 'prefers-reduced-transparency', value: 'no-preference' },
            ],
          });
          await page.addInitScript({ content: pageScript() });
          await routeEngine(page, state.fakes);
          await open(page, state);
          // `VC_AUDIT_SHOTS=<dir>` also keeps a screenshot of each state, for review by eye
          const shots = process.env.VC_AUDIT_SHOTS;
          if (shots) {
            const path = `${shots}/${scheme}-${size.width}/${state.name}.png`;
            await page.screenshot({ path, animations: 'disabled' });
          }
          const blank = await context.newPage();
          const found = await audit(page, blank);
          expect(found, found.join('\n')).toEqual([]);
        });
      }
    });
  }
}
