import { isRedirect } from '@sveltejs/kit';
import { describe, expect, it } from 'vitest';
import { load as root } from '../../routes/+page.ts';
import { load as editor } from '../../routes/editor/+page.ts';
import { load as insights } from '../../routes/insights/+page.ts';

/** Where `load` redirects to, given the page's URL. */
function target(load: (event: never) => unknown, href: string): string | undefined {
  try {
    load({ url: new URL(href, 'http://vc.test') } as never);
  } catch (error) {
    if (isRedirect(error)) return error.location;
    throw error;
  }
  return undefined;
}

describe('redirects', () => {
  it('opens in Client', () => {
    expect(target(root, '/')).toBe('/client');
  });

  it('sends the old editor URL under Tests, keeping the file', () => {
    expect(target(editor, '/editor?file=a.test.yaml')).toBe('/tests/editor?file=a.test.yaml');
  });

  it('sends the old insights URL to Runs › Overview', () => {
    expect(target(insights, '/insights')).toBe('/runs/overview');
  });
});
