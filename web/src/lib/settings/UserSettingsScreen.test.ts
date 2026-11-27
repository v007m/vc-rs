import type { MessageInitShape } from '@bufbuild/protobuf';
import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import type { WriteUserSettingsRequestSchema } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { Toasts } from '#lib/ui/toasts.svelte.ts';
import { MASKED } from './port.ts';
import { UserSettings, userSettingsPort } from './userSettings.svelte.ts';
import UserSettingsScreen from './UserSettingsScreen.svelte';

type Write = MessageInitShape<typeof WriteUserSettingsRequestSchema>;

const before = 'defaults:\n  timeout: 12s\nvars:\n  token: ••••\n';

async function rendered(refuse?: ConnectError) {
  const writes: Write[] = [];
  let reads = 0;
  const client = fakeEngine({
    readUserSettings: () => {
      reads += 1;
      return {
        path: '~/.config/vibranium/config.yaml',
        digest: 'f1',
        settings: [
          { key: 'defaults.timeout', value: '12s', shadowed: true },
          { key: 'vars.token', value: MASKED },
        ],
      };
    },
    writeUserSettings: (request) => {
      const { write, expectedDigest } = request;
      writes.push({
        edits: request.edits.map(({ key, value }) => ({ key, value })),
        write,
        expectedDigest,
      });
      if (refuse) throw refuse;
      return { path: '~/.config/vibranium/config.yaml', before, after: 'x', digest: 'f1' };
    },
  });
  const settings = new UserSettings(userSettingsPort(client));
  await settings.load();
  const toasts = new Toasts(() => undefined);
  render(UserSettingsScreen, { settings, toasts });
  return { writes, reads: () => reads, toasts, user: userEvent.setup() };
}

describe('UserSettingsScreen', () => {
  it('shows the file, each key and value, and the shadowed tag', async () => {
    await rendered();
    expect(screen.getByText('~/.config/vibranium/config.yaml')).toBeTruthy();
    const row = screen.getByRole('row', { name: /defaults\.timeout/ });
    expect(row.textContent).toContain('12s');
    const tag = within(row).getByText('Shadowed by the project');
    expect(tag.getAttribute('title')).toMatch(/project's value wins/);
  });

  it('shows a secret masked and replaces it without showing it', async () => {
    const { user } = await rendered();
    const row = screen.getByRole('row', { name: /vars\.token/ });
    expect(row.textContent).toContain(`${MASKED} (secret, masked)`);
    expect(within(row).queryByText('Shadowed by the project')).toBeNull();
    await user.click(within(row).getByRole('button', { name: 'Replace vars.token' }));
    expect((screen.getByRole('textbox', { name: 'Key' }) as HTMLInputElement).value).toBe(
      'vars.token',
    );
    expect((screen.getByRole('textbox', { name: 'Value' }) as HTMLInputElement).value).toBe('');
  });

  it('refuses a key a client may not set, inline', async () => {
    const { user } = await rendered();
    await user.type(screen.getByRole('textbox', { name: 'Key' }), 'plugins.allowUnconfined');
    await user.click(screen.getByRole('button', { name: 'Set' }));
    expect(screen.getByText(/Use profile, defaults/)).toBeTruthy();
    expect(screen.queryByText(/unsaved/)).toBeNull();
  });

  it('previews the edits, then saves them with the digest and reads the file again', async () => {
    const { user, writes, reads, toasts } = await rendered();
    await user.click(screen.getByRole('button', { name: 'Edit defaults.timeout' }));
    const value = screen.getByRole('textbox', { name: 'Value' });
    await user.clear(value);
    await user.type(value, '30s');
    await user.click(screen.getByRole('button', { name: 'Set' }));
    await user.click(screen.getByRole('button', { name: 'Remove vars.token' }));
    expect(screen.getByRole('status').textContent).toContain('2 unsaved changes.');
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    const preview = await screen.findByRole('region', { name: 'Preview of your settings' });
    expect(preview.textContent).toContain('~/.config/vibranium/config.yaml');
    await user.click(within(preview).getByRole('button', { name: 'Save' }));
    const edits = [{ key: 'defaults.timeout', value: '30s' }, { key: 'vars.token' }];
    expect(writes).toEqual([
      expect.objectContaining({ edits, write: false }),
      expect.objectContaining({ edits, write: true, expectedDigest: 'f1' }),
    ]);
    expect(toasts.list.map((toast) => toast.text)).toEqual(['Saved your settings.']);
    expect(reads()).toBe(2);
    expect(screen.queryByRole('region', { name: 'Preview of your settings' })).toBeNull();
  });

  it('shows the engine refusal inline', async () => {
    const refusal = new ConnectError('edit 1: profile names no profile', Code.InvalidArgument);
    const { user } = await rendered(refusal);
    await user.type(screen.getByRole('textbox', { name: 'Key' }), 'profile');
    await user.type(screen.getByRole('textbox', { name: 'Value' }), 'nope');
    await user.click(screen.getByRole('button', { name: 'Set' }));
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    expect((await screen.findByRole('alert')).textContent).toBe('edit 1: profile names no profile');
  });

  it('is a quiet empty state on an engine without the RPCs', async () => {
    const settings = new UserSettings(userSettingsPort(fakeEngine({})));
    await settings.load();
    render(UserSettingsScreen, { settings });
    expect(screen.getByText('Your settings are not available')).toBeTruthy();
  });
});
