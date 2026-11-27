import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { ShellState } from '#lib/shell/state.svelte.ts';
import Appearance from './Appearance.svelte';

describe('Appearance', () => {
  it('changes theme, accent, density and motion at once', async () => {
    const user = userEvent.setup();
    const shell = new ShellState({ getItem: () => null, setItem: () => undefined });
    render(Appearance, { shell });
    const theme = screen.getByRole('group', { name: 'Theme' });
    expect(theme.querySelector('[aria-pressed=true]')?.textContent).toBe('Match the system');
    await user.click(screen.getByRole('button', { name: 'Dark' }));
    await user.click(screen.getByRole('radio', { name: 'Tide' }));
    await user.click(screen.getByRole('button', { name: 'Compact' }));
    await user.click(screen.getByRole('checkbox', { name: 'Reduce motion' }));
    expect(shell.prefs.appearance).toEqual({
      theme: 'dark',
      accent: 'tide',
      density: 'compact',
      motion: 'reduced',
    });
    expect(screen.getByRole('radio', { name: 'Tide' })).toHaveProperty('checked', true);
  });
});
