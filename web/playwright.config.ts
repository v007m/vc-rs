import { defineConfig, devices } from '@playwright/test';

// The suite runs against `vc ui --no-open` over a seed project (G11, G17). It stays off until
// `vc ui` serves the build (C5): set VC_E2E=1 to start vc ui in the global setup and run the
// specs. vc ui only accepts its own host, so the base URL is 127.0.0.1, not localhost.
const enabled = process.env.VC_E2E === '1';

export default defineConfig({
  testDir: 'tests',
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: 'http://127.0.0.1:7420',
    trace: 'retain-on-failure',
    // The session cookie the global setup saved after opening vc ui's token URL.
    ...(enabled && { storageState: 'tests/.auth/state.json' }),
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  ...(enabled && { globalSetup: './tests/global-setup.ts' }),
});
