import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import type { EstimatePort } from './estimate.ts';
import RunDialog from './RunDialog.svelte';
import type { RunInit } from './session.svelte.ts';

function setup(profile: string | undefined = 'staging', estimate?: EstimatePort) {
  const onrun = vi.fn<(init: RunInit) => void>();
  const onclose = vi.fn<() => void>();
  const asked: Parameters<EstimatePort>[0][] = [];
  const counted: EstimatePort = (selection) => {
    asked.push(selection);
    return Promise.resolve({ tests: 12, files: 3, skipped: 1 });
  };
  render(RunDialog, {
    profiles: ['staging', 'prod-eu'],
    profile,
    selector: 'tag:smoke',
    estimate: estimate ?? counted,
    onrun,
    onclose,
  });
  return { onrun, onclose, asked, user: userEvent.setup() };
}

it('opens titled Run tests with the selector and profile it was given', () => {
  setup();
  expect(screen.getByRole('dialog', { name: 'Run tests' })).toBeTruthy();
  expect(screen.getByRole<HTMLInputElement>('textbox', { name: 'Selector' }).value).toBe(
    'tag:smoke',
  );
  expect(screen.getByRole<HTMLSelectElement>('combobox', { name: 'Profile' }).value).toBe(
    'staging',
  );
});

it('runs the request the form describes', async () => {
  const { onrun, user } = setup();
  const selector = screen.getByRole('textbox', { name: 'Selector' });
  await user.clear(selector);
  await user.type(selector, 'users.crud#a services/orders/');
  await user.click(screen.getByRole('checkbox', { name: 'Stop at the first failure' }));
  await user.click(screen.getByRole('button', { name: 'Run' }));
  expect(onrun).toHaveBeenCalledWith({
    selection: { ids: ['users.crud#a'], paths: ['services/orders/'], tags: [] },
    profiles: ['staging'],
    bail: true,
    strict: false,
    reports: [],
  });
});

it('adds Strict and the report files from More options to the request and command', async () => {
  const { onrun, user } = setup();
  await user.click(screen.getByText('More options'));
  await user.click(screen.getByRole('checkbox', { name: 'Strict' }));
  await user.click(screen.getByRole('checkbox', { name: 'junit' }));
  expect(screen.getByLabelText('Command').textContent).toBe(
    'vc run -t smoke --profile staging --strict --report junit',
  );
  await user.click(screen.getByRole('button', { name: 'Run' }));
  expect(onrun.mock.calls[0]?.[0]).toMatchObject({ strict: true, reports: ['junit'] });
});

it('shows what the selection would run once the typing settles', async () => {
  const { asked } = setup();
  await vi.waitFor(() => expect(screen.getByText('12 tests · 3 files · 1 skipped')).toBeTruthy());
  expect(asked).toEqual([{ ids: [], paths: [], tags: ['smoke'] }]);
});

it('says why the plan could not be counted', async () => {
  setup('staging', () => Promise.reject(new Error('project does not compile')));
  await vi.waitFor(() => expect(screen.getByText('project does not compile')).toBeTruthy());
});

it('warns that a production profile sends real requests', async () => {
  const { user } = setup();
  expect(screen.queryByRole('note')).toBeNull();
  await user.selectOptions(screen.getByRole('combobox', { name: 'Profile' }), 'prod-eu');
  expect(screen.getByRole('note').textContent).toContain(
    'This run sends real requests to prod-eu.',
  );
});

it('closes on Cancel without running', async () => {
  const { onrun, onclose, user } = setup();
  await user.click(screen.getByRole('button', { name: 'Cancel' }));
  expect(onclose).toHaveBeenCalledOnce();
  expect(onrun).not.toHaveBeenCalled();
});
