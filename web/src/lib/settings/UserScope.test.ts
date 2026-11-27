import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import type { SettingsScope } from './userScope.ts';
import UserScope from './UserScope.svelte';

function pressed(): string[] {
  return screen
    .getAllByRole('button')
    .filter((button) => button.getAttribute('aria-pressed') === 'true')
    .map((button) => button.textContent ?? '');
}

describe('UserScope', () => {
  it.each([
    ['/settings', ['Project']],
    ['/settings/alerts', ['Project']],
    ['/settings/plugins', ['Project']],
    ['/settings/user', ['You only']],
    ['/settings/appearance', []],
  ])('on %s presses %j', (pathname, want) => {
    render(UserScope, { pathname, onchoose: () => undefined });
    expect(screen.getByRole('group', { name: 'Settings scope' })).toBeTruthy();
    expect(pressed()).toEqual(want);
  });

  it('chooses the scope pressed', async () => {
    const chosen: SettingsScope[] = [];
    render(UserScope, { pathname: '/settings', onchoose: (scope) => chosen.push(scope) });
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'You only' }));
    await user.click(screen.getByRole('button', { name: 'Project' }));
    expect(chosen).toEqual(['user', 'project']);
  });
});
