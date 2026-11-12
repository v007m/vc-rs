import { defineConfig, devices } from '@playwright/test';

// Layout checks against the built site: run `pnpm build` first, then `pnpm test`. The web server
// is astro preview over `dist`, on its own port so it never meets a preview someone already runs.
const PORT = 4391;

export default defineConfig({
  testDir: 'tests',
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? 'github' : 'list',
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    reducedMotion: 'reduce',
    trace: 'retain-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: `pnpm exec astro preview --ignore-lock --host 127.0.0.1 --port ${PORT}`,
    url: `http://127.0.0.1:${PORT}/vc-rs/`,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
