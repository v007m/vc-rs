import { describe, expect, it, vi } from 'vitest';
import { defaultAppearance } from './appearance.ts';
import { commandItems, type CommandContext } from './commands.ts';
import { filterPalette } from './palette.ts';

function context(over: Partial<CommandContext> = {}): CommandContext {
  return {
    runHref: '/runs?run=ask',
    fileHref: (path) => `/tests/editor?file=${path}`,
    files: ['services/users/users.api.yaml'],
    profiles: ['dev', 'staging'],
    profile: 'dev',
    appearance: defaultAppearance,
    useProfile: vi.fn(),
    setAppearance: vi.fn(),
    say: vi.fn(),
    ...over,
  };
}

describe('palette commands', () => {
  it('offers a run, the other profiles, the other themes, density and motion', () => {
    const labels = commandItems(context())
      .filter((item) => !item.searchOnly)
      .map((item) => item.label);
    expect(labels).toEqual([
      'Run tests…',
      'Use profile staging',
      'Theme: Light',
      'Theme: Dark',
      'Density: Compact',
      'Reduce motion',
    ]);
  });

  it('switches the profile and says so', () => {
    const ctx = context();
    commandItems(ctx)
      .find((item) => item.id === 'profile:staging')
      ?.run?.();
    expect(ctx.useProfile).toHaveBeenCalledWith('staging');
    expect(ctx.say).toHaveBeenCalledWith('Using profile staging for this session.');
  });

  it('changes the appearance and says so', () => {
    const ctx = context({ appearance: { ...defaultAppearance, motion: 'reduced' } });
    const motion = commandItems(ctx).find((item) => item.id === 'motion:system');
    expect(motion?.label).toBe('Motion: match the system');
    motion?.run?.();
    expect(ctx.setAppearance).toHaveBeenCalledWith({ motion: 'system' });
    expect(ctx.say).toHaveBeenCalledWith('Motion follows the system.');
  });

  it('lists files only for a search, each opening the editor', () => {
    const items = commandItems(context());
    expect(filterPalette(items, '').some((group) => group.group === 'Files')).toBe(false);
    const found = filterPalette(items, 'open users').flatMap((group) => group.items);
    expect(found.map((item) => item.href)).toContain(
      '/tests/editor?file=services/users/users.api.yaml',
    );
  });
});
