import { defineConfig, devices } from '@playwright/test';

// The suite runs against `vc ui --no-open` over a seed project (G11, G17). It stays off until
// `vc ui` serves the build (C5): set VC_E2E=1 to start the server and run the specs.
const enabled = process.env.VC_E2E === '1';
const port = 7420;

export default defineConfig({
  testDir: 'tests',
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: `http://127.0.0.1:${port}`,
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  ...(enabled && {
    webServer: {
      command: `vc ui --no-open --port ${port} --profile dev`,
      cwd: 'tests/fixtures/seed',
      url: `http://127.0.0.1:${port}`,
      reuseExistingServer: !process.env.CI,
    },
  }),
});
