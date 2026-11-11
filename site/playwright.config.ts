import { defineConfig, devices } from '@playwright/test';

// The playground's Run button, end to end, as web/'s suite is set up: Chromium against the built
// site, served under the Pages base path by tests/serve.mjs. Build the runner and the site first:
// `pnpm --filter site wasm && pnpm --filter site build`.
const port = 4329;

export default defineConfig({
  testDir: 'tests',
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: `http://127.0.0.1:${port}/vc-rs/`,
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: `node tests/serve.mjs ${port}`,
    url: `http://127.0.0.1:${port}/vc-rs/playground/`,
    reuseExistingServer: !process.env.CI,
  },
});
