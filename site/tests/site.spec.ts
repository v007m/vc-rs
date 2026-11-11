import { expect, test, type Page } from '@playwright/test';

// Layout checks on the built site: the landing page's window cards, the header's search and tabs,
// and that no page scrolls sideways on a phone.
const BASE = '/vc-rs';
const DESKTOP = { width: 1440, height: 900 };
const PHONE = { width: 390, height: 844 };
const PAGES = [
  `${BASE}/`,
  `${BASE}/start/install/`,
  `${BASE}/start/quickstart/`,
  `${BASE}/start/concepts/`,
  `${BASE}/reference/cli/`,
  `${BASE}/guides/`,
];

for (const theme of ['dark', 'light']) {
  test.describe(`${theme} theme`, () => {
    test.beforeEach(async ({ page }) => {
      await page.addInitScript((t) => localStorage.setItem('starlight-theme', t), theme);
    });

    test('window cards in a row share one height', async ({ page }) => {
      await page.setViewportSize(DESKTOP);
      await page.goto(`${BASE}/`);
      const rows = page.locator('.lp-window-row');
      expect(await rows.count()).toBeGreaterThan(0);
      for (const row of await rows.all()) {
        const heights = await row
          .locator('.lp-window')
          .evaluateAll((els) => els.map((e) => Math.round(e.getBoundingClientRect().height)));
        expect(heights.length).toBe(3);
        expect(Math.max(...heights) - Math.min(...heights)).toBeLessThanOrEqual(1);
      }
    });

    for (const path of PAGES) {
      test(`no horizontal scroll at 390px: ${path}`, async ({ page }) => {
        await page.setViewportSize(PHONE);
        await page.goto(path);
        const [scroll, client] = await page.evaluate(() => [
          document.documentElement.scrollWidth,
          document.documentElement.clientWidth,
        ]);
        expect(scroll).toBeLessThanOrEqual(client);
      });
    }
  });
}

/** How far the search box's centre sits from the viewport's centre, in pixels. */
async function searchOffset(page: Page): Promise<number> {
  const box = await page.locator('.vc-search site-search button').first().boundingBox();
  expect(box).not.toBeNull();
  return Math.abs(box!.x + box!.width / 2 - DESKTOP.width / 2);
}

for (const path of [`${BASE}/`, `${BASE}/start/quickstart/`, `${BASE}/reference/cli/`]) {
  test(`search is centred in the header: ${path}`, async ({ page }) => {
    await page.setViewportSize(DESKTOP);
    await page.goto(path);
    expect(await searchOffset(page)).toBeLessThanOrEqual(2);
  });
}

test('landing tabs point at sections on the page', async ({ page }) => {
  await page.setViewportSize(DESKTOP);
  await page.goto(`${BASE}/`);
  const hrefs = await page.locator('.vc-tabs a').evaluateAll((as) => as.map((a) => a.getAttribute('href') ?? ''));
  const anchors = hrefs.filter((h) => h.startsWith('#'));
  expect(anchors.length).toBeGreaterThan(0);
  for (const href of anchors) {
    await expect(page.locator(href), href).toHaveCount(1);
  }
  for (const href of hrefs.filter((h) => !h.startsWith('#'))) {
    expect((await page.request.get(href)).status(), href).toBe(200);
  }
});

test('docs area tabs resolve and mark the current area', async ({ page }) => {
  await page.setViewportSize(DESKTOP);
  await page.goto(`${BASE}/reference/cli/`);
  const tabs = page.locator('.vc-tabs[aria-label="Docs areas"] a');
  expect(await tabs.count()).toBeGreaterThan(0);
  for (const href of await tabs.evaluateAll((as) => as.map((a) => a.getAttribute('href') ?? ''))) {
    expect((await page.request.get(href)).status(), href).toBe(200);
  }
  await expect(page.locator('.vc-tabs a[aria-current]')).toHaveText('CLI');
});
