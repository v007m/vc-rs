import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import type { Maybe } from '#lib/api/maybe.ts';
import { Toasts } from '#lib/ui/toasts.svelte.ts';
import Onboarding from './Onboarding.svelte';
import { initNotYet, type DemoPlan, type OnboardingPort, type Planned } from './port.ts';

const files = [
  { path: 'vibranium.config.yaml', before: '', after: 'name: shop\n' },
  { path: 'services/users/users.api.yaml', before: '', after: 'id: users\n' },
];
const planned: Planned = { files, digest: 'd1', root: '~/dev/shop' };
const demo: DemoPlan = {
  files,
  digest: 'd2',
  root: '~/dev/shop',
  running: false,
  url: 'http://localhost:7421',
  start: 'vc demo start --detach',
};
const ok = <T>(value: T): Promise<Maybe<T>> => Promise.resolve({ available: true, value });

function setup(port: Partial<OnboardingPort> = {}) {
  const full: OnboardingPort = {
    init: vi.fn(() => ok(planned)),
    demo: vi.fn(() => ok(demo)),
    ...port,
  };
  const project = { load: vi.fn(() => Promise.resolve()) };
  const toasts = new Toasts(() => 0);
  const user = userEvent.setup();
  render(Onboarding, { port: full, project, toasts });
  return { user, port: full, project, toasts };
}

describe('Onboarding', () => {
  it('offers setting up this folder first, and the demo', () => {
    setup();
    expect(screen.getByRole('heading', { name: 'Get started with Vibranium' })).toBeTruthy();
    const init = screen.getByRole('button', { name: 'Set up this folder' });
    expect(init.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: 'Try the demo' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
    expect(screen.getByRole('form', { name: 'Set up this folder' })).toBeTruthy();
    expect(screen.getByRole('checkbox', { name: 'Add a sample test' })).toHaveProperty(
      'checked',
      true,
    );
  });

  it('previews the files, writes them with the digest, then toasts and reloads', async () => {
    const { user, port, project, toasts } = setup();
    await user.type(screen.getByRole('textbox', { name: 'Project name' }), 'shop');
    await user.type(screen.getByRole('textbox', { name: 'First service' }), 'users');
    await user.click(screen.getByRole('checkbox', { name: 'Add a sample test' }));
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    expect(port.init).toHaveBeenLastCalledWith({
      name: 'shop',
      service: 'users',
      baseUrl: '',
      profile: '',
      sample: false,
    });
    expect(await screen.findByText('services/users/users.api.yaml')).toBeTruthy();
    expect(screen.getByText('2 files change.')).toBeTruthy();
    expect(screen.getByText('~/dev/shop')).toBeTruthy();
    expect(project.load).not.toHaveBeenCalled();

    await user.click(screen.getByRole('button', { name: 'Write files' }));
    expect(port.init).toHaveBeenLastCalledWith(expect.objectContaining({ name: 'shop' }), 'd1');
    expect(project.load).toHaveBeenCalledOnce();
    expect(toasts.list.map((t) => t.text)).toEqual(['Set up shop in ~/dev/shop']);
  });

  it('names the project after its folder when no name was given', async () => {
    const { user, toasts } = setup();
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    await user.click(await screen.findByRole('button', { name: 'Write files' }));
    expect(toasts.list.map((t) => t.text)).toEqual(['Set up shop in ~/dev/shop']);
  });

  it('drops a preview when an answer changes, so nothing stale is written', async () => {
    const { user } = setup();
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    expect(await screen.findByRole('button', { name: 'Write files' })).toBeTruthy();
    await user.type(screen.getByRole('textbox', { name: 'Base URL' }), 'http://localhost:9000');
    expect(screen.queryByRole('button', { name: 'Write files' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Preview' })).toBeTruthy();
  });

  it('shows a failed write inline and as a toast, without reloading', async () => {
    const failing = vi.fn((_: unknown, digest?: string) =>
      digest
        ? Promise.reject(new ConnectError('the folder changed since the preview', Code.Aborted))
        : ok(planned),
    );
    const { user, project, toasts } = setup({ init: failing });
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    await user.click(await screen.findByRole('button', { name: 'Write files' }));
    expect((await screen.findByRole('alert')).textContent).toBe(
      'the folder changed since the preview',
    );
    expect(toasts.list).toMatchObject([
      { tone: 'error', text: 'the folder changed since the preview' },
    ]);
    expect(project.load).not.toHaveBeenCalled();
  });

  it('says quietly to run vc init when this vc cannot set up a project here', async () => {
    const { user, toasts } = setup({
      init: () => Promise.resolve({ available: false, reason: initNotYet }),
    });
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    expect((await screen.findByRole('status')).textContent).toBe(initNotYet);
    expect(screen.queryByRole('alert')).toBeNull();
    expect(toasts.list).toEqual([]);
  });

  it('previews the demo, shows the start command to copy, and writes with the digest', async () => {
    const { user, port, project, toasts } = setup();
    await user.click(screen.getByRole('button', { name: 'Try the demo' }));
    await user.click(screen.getByRole('button', { name: 'Preview the demo' }));
    expect(port.demo).toHaveBeenLastCalledWith();
    expect(await screen.findByText('vc demo start --detach')).toBeTruthy();
    expect(screen.getByText('2 files change.')).toBeTruthy();

    await user.click(screen.getByRole('button', { name: 'Copy' }));
    expect(await navigator.clipboard.readText()).toBe('vc demo start --detach');
    expect(screen.getByText('Copied.')).toBeTruthy();

    await user.click(screen.getByRole('button', { name: 'Write files' }));
    expect(port.demo).toHaveBeenLastCalledWith('d2');
    expect(project.load).toHaveBeenCalledOnce();
    expect(toasts.list).toMatchObject([
      { text: 'Wrote the demo project in ~/dev/shop', action: { label: 'Copy command' } },
    ]);
    expect(screen.getByText('vc demo start --detach')).toBeTruthy();
  });

  it('shows where the demo answers when its service is running, and never a start command', async () => {
    const { user, toasts } = setup({ demo: () => ok({ ...demo, running: true }) });
    await user.click(screen.getByRole('button', { name: 'Try the demo' }));
    await user.click(screen.getByRole('button', { name: 'Preview the demo' }));
    expect(await screen.findByText('http://localhost:7421')).toBeTruthy();
    expect(screen.queryByText('vc demo start --detach')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Write files' }));
    expect(toasts.list[0]?.action).toBeUndefined();
  });
});
