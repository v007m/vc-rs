import type { Page } from '@playwright/test';

/** One screen the audit checks: a route, and what to do there before checking it. */
export type State = { name: string; path: string; act?: (page: Page) => Promise<void> };

const click = (page: Page, name: RegExp | string) =>
  page
    .getByRole('button', { name, exact: typeof name === 'string' })
    .first()
    .click();

/** Shows the sidebar, a drawer at 1024 px. */
async function sidebar(page: Page): Promise<void> {
  const drawer = page.getByRole('button', { name: /show sidebar/i });
  if (await drawer.isVisible()) await drawer.click();
}

/** Opens the users.crud create request in the Client; at 1024 px the sidebar is a drawer. */
async function openRequest(page: Page): Promise<void> {
  await sidebar(page);
  await page.getByRole('treeitem', { name: 'Creates a user with the given email' }).click();
  const scrim = page.locator('.scrim-drawer');
  if (await scrim.isVisible()) await scrim.click({ position: { x: 900, y: 400 } });
}

/** Every route, and each overlay over the route it opens on. */
export const states: State[] = [
  { name: 'client', path: '/client' },
  {
    name: 'client-response',
    path: '/client',
    act: async (page) => {
      await openRequest(page);
      await click(page, 'Send');
    },
  },
  {
    name: 'client-layout-menu',
    path: '/client',
    act: async (page) => {
      await openRequest(page);
      await click(page, 'Layout');
    },
  },
  {
    name: 'client-timing',
    path: '/client',
    act: async (page) => {
      await openRequest(page);
      await click(page, 'Send');
      await page.getByRole('tab', { name: /^Body/ }).first().click();
      await page
        .getByRole('region', { name: 'Response' })
        .getByRole('tab', { name: /^Timing/ })
        .click();
    },
  },
  {
    name: 'client-stacked',
    path: '/client',
    act: async (page) => {
      await openRequest(page);
      await click(page, 'Layout');
      await page.getByRole('menuitem', { name: 'Stacked' }).click();
    },
  },
  { name: 'palette', path: '/client', act: (page) => page.locator('.search-btn').click() },
  { name: 'tests', path: '/tests' },
  {
    name: 'tests-git-menu',
    path: '/tests',
    act: async (page) => {
      await sidebar(page);
      await click(page, 'More git actions');
    },
  },
  {
    name: 'tests-git-diff',
    path: '/tests',
    act: async (page) => {
      await sidebar(page);
      await click(page, /crud\.test\.yaml/);
    },
  },
  { name: 'editor', path: '/tests/editor?file=services/users/crud.test.yaml' },
  {
    name: 'editor-file-menu',
    path: '/tests/editor?file=services/users/crud.test.yaml',
    act: (page) => click(page, 'File actions'),
  },
  {
    name: 'editor-rename',
    path: '/tests/editor?file=services/users/crud.test.yaml',
    act: async (page) => {
      await click(page, 'File actions');
      await page.getByRole('menuitem', { name: 'Rename or move…' }).click();
    },
  },
  {
    name: 'editor-save-anyway',
    path: '/tests/editor?file=services/users/crud.test.yaml',
    act: async (page) => {
      await page.locator('.cm-line').nth(10).click();
      await page.keyboard.press('End');
      await page.keyboard.type('X');
      await page.waitForTimeout(600);
      await click(page, 'Save');
    },
  },
  { name: 'runs', path: '/runs' },
  { name: 'runs-row-menu', path: '/runs', act: (page) => click(page, /Run #\d+ actions/) },
  { name: 'run-dialog', path: '/runs', act: (page) => click(page, /^Run…/) },
  {
    name: 'run-dialog-more',
    path: '/runs',
    act: async (page) => {
      await click(page, /^Run…/);
      await page.locator('details.more summary').click();
    },
  },
  { name: 'stored-run', path: '/runs/18' },
  {
    name: 'stored-run-exchanges',
    path: '/runs/18',
    act: (page) =>
      page
        .getByRole('tab', { name: /Exchanges/ })
        .first()
        .click(),
  },
  { name: 'stored-run-export', path: '/runs/18', act: (page) => click(page, /Export run/) },
  { name: 'compare', path: '/runs/compare/17/18' },
  { name: 'load', path: '/runs/load' },
  { name: 'load-live', path: '/runs/load', act: (page) => click(page, 'Start') },
  {
    name: 'load-report',
    path: '/runs/load',
    act: async (page) => {
      await click(page, 'Start');
      await click(page, 'Report');
    },
  },
  { name: 'load-stored', path: '/runs/load/15' },
  { name: 'overview', path: '/runs/overview' },
  { name: 'live', path: '/runs/live' },
  { name: 'agents', path: '/agents' },
  { name: 'settings', path: '/settings' },
];
