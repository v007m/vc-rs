import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { expect, test, type Page } from '@playwright/test';
import { requiresVcUi } from './support';

// The HTML report (D169) opened from disk, as a reader opens it: no server, no network. The
// reports are the synthetic run of `vc-adapter-reporters`' `html_sample` example, one test in
// ten failed with an exchange.
requiresVcUi();

const root = fileURLToPath(new URL('../..', import.meta.url));
const made: string[] = [];

/** Writes the sample report of `tests` tests and returns the URL of its page. */
function sample(tests: number, layout: 'folder' | 'single'): string {
  const dir = mkdtempSync(join(tmpdir(), 'vc-report-'));
  made.push(dir);
  const args = ['run', '-q', '-p', 'vc-adapter-reporters', '--example', 'html_sample', '--'];
  execFileSync('cargo', [...args, dir, String(tests), layout], { cwd: root, stdio: 'inherit' });
  return pathToFileURL(join(dir, layout === 'folder' ? 'index.html' : 'report.html')).href;
}

test.afterAll(() => made.forEach((dir) => rmSync(dir, { recursive: true, force: true })));

/** Every URL the page asks for, to show that the report never leaves the disk. */
function requests(page: Page): string[] {
  const urls: string[] = [];
  page.on('request', (request) => urls.push(request.url()));
  return urls;
}

/** The time between two `performance` marks the report sets, in ms. */
async function measured(page: Page, name: string): Promise<number> {
  await page.waitForFunction((n) => performance.getEntriesByName(n).length > 0, name);
  return page.evaluate((n) => performance.getEntriesByName(n)[0]?.duration ?? Infinity, name);
}

test.describe('a 10,000-test report in folders', () => {
  let url = '';
  test.beforeAll(() => {
    url = sample(10_000, 'folder');
  });

  test('paints within a second and opens a test within 100 ms', async ({ page }) => {
    const urls = requests(page);
    await page.goto(url);
    await expect(page.getByText('10000 of 10000 tests')).toBeVisible();
    const painted = await page.evaluate(
      () => performance.getEntriesByName('vib:painted')[0]?.startTime ?? Infinity,
    );
    expect(painted).toBeLessThan(1000);
    await page.getByRole('option', { name: /Checks thing 9\b/ }).click();
    await expect(page.getByRole('heading', { name: 'Checks thing 9' })).toBeVisible();
    expect(await measured(page, 'vib:open')).toBeLessThan(100);
    expect(urls.every((u) => u.startsWith('file:'))).toBe(true);
  });

  test('shows the run, its totals and a failed test with its check and exchange', async ({
    page,
  }) => {
    const dialogs: string[] = [];
    page.on('dialog', (dialog) => {
      dialogs.push(dialog.message());
      void dialog.dismiss();
    });
    await page.goto(url);
    await expect(page.getByRole('heading', { name: 'Run #1' })).toBeVisible();
    await expect(page.getByRole('img', { name: /9000 passed, 1000 failed/ })).toBeVisible();
    await page.goto(`${url}#/test/${encodeURIComponent('Checks thing 9')}`);
    const detail = page.getByRole('region', { name: 'Checks thing 9' });
    await expect(detail.getByRole('list', { name: 'Checks' })).toContainText('expected 200');
    await expect(detail.getByRole('list', { name: 'Checks' })).toContainText('actual 500');
    const exchange = detail.getByText('GET http://localhost/users/9');
    await exchange.click();
    // The sample's body holds markup; it must show as text and never run.
    await expect(detail.getByText('"error": "</script><script>alert(1)</script>"')).toBeVisible();
    expect(dialogs).toEqual([]);
  });

  test('filters by text and by status, and moves through the list from the keyboard', async ({
    page,
  }) => {
    await page.goto(url);
    await page
      .getByRole('group', { name: 'Status' })
      .getByRole('button', { name: 'failed' })
      .click();
    await expect(page.getByText('1000 of 10000 tests')).toBeVisible();
    await page.getByRole('searchbox', { name: 'Filter tests' }).fill('thing 99');
    await expect(page.getByText(/^\d+ of 10000 tests$/)).toHaveText('12 of 10000 tests');
    const list = page.getByRole('listbox', { name: 'Tests' });
    // Failed tests matching "thing 99": 99, 999, 9909, … — ArrowDown moves from the first to the second.
    await list.focus();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading', { name: 'Checks thing 999' })).toBeVisible();
  });
});

test('a single-file report inflates its data and opens a test', async ({ page }) => {
  const urls = requests(page);
  await page.goto(sample(200, 'single'));
  await expect(page.getByText('200 of 200 tests')).toBeVisible();
  await page.getByRole('option', { name: /Checks thing 19\b/ }).click();
  const detail = page.getByRole('region', { name: 'Checks thing 19' });
  await detail.getByText('GET http://localhost/users/19').click();
  await expect(detail.getByText('"user": 19')).toBeVisible();
  expect(urls.every((u) => u.startsWith('file:'))).toBe(true);
});
