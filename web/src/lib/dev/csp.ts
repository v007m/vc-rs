import { createHash } from 'node:crypto';

/**
 * The inline `style` of SvelteKit's route announcer (`#svelte-announcer`, its root.svelte):
 * the only inline style the app has. The CSP allows exactly it, by its hash (D311).
 */
export const announcerStyle =
  'position: absolute; left: 0; top: 0; clip: rect(0 0 0 0); clip-path: inset(50%); overflow: hidden; white-space: nowrap; width: 1px; height: 1px';

/** The CSP source that allows one inline style attribute: `sha256-` and its base64 SHA-256. */
export function styleHash(style: string): `sha256-${string}` {
  return `sha256-${createHash('sha256').update(style).digest('base64')}`;
}
