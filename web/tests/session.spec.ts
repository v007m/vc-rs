import { expect, test } from '@playwright/test';
import { fromSetup, requiresVcUi } from './support';

requiresVcUi();

// No session cookie: these requests come from someone who holds only what vc ui printed.
test.use({ storageState: { cookies: [], origins: [] } });

test('opens the launch link only once, then says how to get in', async ({ page }) => {
  const response = await page.goto(fromSetup('VC_E2E_LAUNCH_URL'));
  expect(response?.status()).toBe(401);
  await expect(page.locator('body')).toHaveText(
    'this sign-in link was used already; use the open tab or restart vc ui',
  );
});

test('refuses the agent token on /api', async ({ request, baseURL }) => {
  const response = await request.post('/api/vibranium.engine.v1.EngineService/GetProject', {
    headers: {
      authorization: `Bearer ${fromSetup('VC_E2E_AGENT_TOKEN')}`,
      'content-type': 'application/proto',
      origin: baseURL ?? '',
    },
    data: Buffer.alloc(0),
  });
  expect(response.status()).toBe(401);
  expect(await response.text()).toBe('a bearer token works only on /mcp');
});
