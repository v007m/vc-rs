import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import Problems from './Problems.svelte';

describe('Problems', () => {
  it('says why the check failed in place of its diagnostics', () => {
    render(Problems, {
      diagnostics: [],
      failure: 'hook timed out',
      live: false,
      ongo: () => {},
      onfix: () => {},
    });
    const panel = screen.getByRole('region', { name: 'Problems' });
    expect(panel.textContent).toContain(
      'The file was saved, but checking it failed: hook timed out',
    );
    expect(screen.queryByText('No problems in this file.')).toBeNull();
  });
});
