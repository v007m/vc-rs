import { expect, test, type APIRequestContext } from '@playwright/test';

// Every image in a docs page's content has alt text and loads, and each screenshot shows only the
// picture for the theme. The pages come from the sitemap, so a new page is checked without being
// listed here.
const BASE = '/vc-rs';

/** The site's page paths, from the sitemap index and the sitemaps it names. */
async function pagePaths(request: APIRequestContext): Promise<string[]> {
  const locs = (xml: string) => [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => new URL(m[1]).pathname);
  const index = await request.get(`${BASE}/sitemap-index.xml`);
  expect(index.status()).toBe(200);
  const pages: string[] = [];
  for (const sitemap of locs(await index.text())) {
    const response = await request.get(sitemap);
    expect(response.status(), sitemap).toBe(200);
    pages.push(...locs(await response.text()));
  }
  return pages;
}

const attribute = (tag: string, name: string) => new RegExp(`\\s${name}="([^"]*)"`).exec(tag)?.[1];

test('every image in the docs content has alt text and loads', async ({ request }) => {
  const pages = await pagePaths(request);
  expect(pages.length).toBeGreaterThan(50);
  const sources = new Map<string, string>();
  for (const path of pages) {
    const html = await (await request.get(path)).text();
    const main = /<main[\s>][\s\S]*<\/main>/.exec(html)?.[0] ?? '';
    for (const [tag] of main.matchAll(/<img\b[^>]*>/g)) {
      const src = attribute(tag, 'src');
      expect(src, `${path}: ${tag}`).toBeTruthy();
      expect(attribute(tag, 'alt')?.trim(), `${path}: alt text of ${src}`).toBeTruthy();
      const srcset = (attribute(tag, 'srcset') ?? '').split(',').map((entry) => entry.trim().split(/\s+/)[0]);
      for (const url of [src!, ...srcset.filter(Boolean)]) sources.set(new URL(url, `http://site${path}`).pathname, path);
    }
  }
  expect(sources.size, 'the screenshots are in the docs').toBeGreaterThan(10);
  for (const [src, path] of sources) {
    const response = await request.get(src);
    expect(response.status(), `${src} on ${path}`).toBe(200);
  }
});

test('each screenshot shows one picture, the one for the theme', async ({ page }) => {
  for (const colorScheme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme });
    await page.goto(`${BASE}/guides/web-ui/`);
    const figures = page.locator('figure.vc-shot');
    expect(await figures.count()).toBeGreaterThan(0);
    for (const figure of await figures.all()) {
      const visible = figure.locator('img:visible');
      await expect(visible).toHaveCount(1);
      await expect(visible).toHaveAttribute('src', new RegExp(`-${colorScheme}\\.`));
    }
  }
});
