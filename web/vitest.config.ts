import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig, mergeConfig } from 'vitest/config';
import viteConfig from './vite.config.ts';

// Unit and component tests run in jsdom with fake engine transports, so they need no `vc ui`.
// The Playwright suite under tests/ is the one that drives a real server.
export default mergeConfig(
  viteConfig,
  defineConfig({
    plugins: [svelteTesting()],
    test: { environment: 'jsdom', include: ['src/**/*.test.ts'] },
  }),
);
