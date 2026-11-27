import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { ProjectStatus } from './project.svelte.ts';
import StatusLine from './StatusLine.svelte';

describe('StatusLine', () => {
  it('shows the git branch once the engine gives it (D532)', () => {
    render(StatusLine, { project: new ProjectStatus(fakeEngine({})), branch: 'feature/users' });
    expect(screen.getByRole('contentinfo', { name: 'Status' }).textContent).toContain(
      'branch feature/users',
    );
  });

  it('leaves the branch out when there is none', () => {
    render(StatusLine, { project: new ProjectStatus(fakeEngine({})) });
    expect(screen.getByRole('contentinfo').textContent).not.toContain('branch');
  });
});
