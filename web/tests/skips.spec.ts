import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';
import { requiresVcUi } from './support';
import { startVcUi, type VcUi } from './vcui';

requiresVcUi();

/**
 * Its own `vc ui` on a port the OS picks, so the expired skip stays out of the seed project the
 * other specs share.
 */
const port = '0';
const crud = 'services/users/crud.test.yaml';

/** Gives `listsUsers` a skip whose day has passed: it runs again and only the line is left. */
async function expireASkip(projectDir: string): Promise<void> {
  const path = join(projectDir, crud);
  const text = await readFile(path, 'utf8');
  const skip = `    call: users#listUsers
    skip:
      reason: waits on the paged list
      until: 2000-01-01
`;
  await writeFile(path, text.replace('    call: users#listUsers\n', skip));
}

let vcUi: VcUi | undefined;
test.beforeAll(async () => {
  vcUi = await startVcUi(port, expireASkip);
});
test.afterAll(async () => {
  await vcUi?.stop();
});

test('lists a skip that has ended and removes it after a preview', async ({ browser }) => {
  if (!vcUi) throw new Error('vc ui did not start');
  const page = await (await browser.newContext()).newPage();
  await page.goto(vcUi.printed.launchUrl);
  await page.goto(`${vcUi.origin}/insights`);
  const panel = page.getByRole('region', { name: 'Skips expiring soon' });
  const row = panel.getByRole('listitem').filter({ hasText: 'users.crud#listsUsers' });
  await expect(row).toContainText('ended 2000-01-01');
  await expect(row).toContainText('waits on the paged list');
  await row.getByRole('button', { name: 'Unskip users.crud#listsUsers' }).click();
  const form = panel.getByRole('group', { name: 'Unskip users.crud#listsUsers' });
  await form.getByRole('button', { name: 'Preview' }).click();
  await expect(form.getByRole('region', { name: `Changes to ${crud}` })).toContainText(
    'until: 2000-01-01',
  );
  await form.getByRole('button', { name: 'Remove skip' }).click();
  await expect(panel.getByText('No skip ends in the next 14 days.')).toBeVisible();
  expect(await readFile(join(vcUi.projectDir, crud), 'utf8')).not.toContain('skip:');
});
