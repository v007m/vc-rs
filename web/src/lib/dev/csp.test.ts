import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { announcerStyle, styleHash } from './csp.ts';

/** SvelteKit's root component, which renders the route announcer; vitest runs in `web/`. */
const root = join(process.cwd(), 'node_modules/@sveltejs/kit/src/runtime/components/root.svelte');

describe('the announcer style the CSP allows', () => {
  it('is the one SvelteKit renders, so an upgrade that changes it fails here', () => {
    expect(readFileSync(root, 'utf8')).toContain(`style="${announcerStyle}"`);
  });

  it('is allowed by the SHA-256 of its exact text, as the browser computes it', () => {
    expect(styleHash(announcerStyle)).toBe('sha256-S8qMpvofolR8Mpjy4kQvEm7m1q8clzU4dfDH0AmvZjo=');
  });
});
