import { defineConfig, devices } from '@playwright/test';

// The style audit (`pnpm audit:style`): the built UI under `vite preview`, the engine API faked
// in the page's routes (audit/engine.ts), so it needs no `vc ui`.
const port = 4174;

export default defineConfig({
  testDir: '.',
  testMatch: 'style.spec.ts',
  forbidOnly: !!process.env.CI,
  reporter: 'line',
  fullyParallel: true,
  outputDir: '../test-results/audit',
  use: { ...devices['Desktop Chrome'], baseURL: `http://localhost:${port}` },
  webServer: {
    command: `pnpm exec vite preview --port ${port} --strictPort`,
    cwd: '..',
    url: `http://localhost:${port}`,
    reuseExistingServer: false,
  },
});
