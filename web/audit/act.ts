import type { Page } from '@playwright/test';

/** Clicks the first button named `name` (a string matches exactly). */
export const click = (page: Page, name: RegExp | string) =>
  page
    .getByRole('button', { name, exact: typeof name === 'string' })
    .first()
    .click();

/** Shows the sidebar, a drawer at 1024 px. */
export async function sidebar(page: Page): Promise<void> {
  const drawer = page.getByRole('button', { name: /show sidebar/i });
  if (await drawer.isVisible()) await drawer.click();
}
