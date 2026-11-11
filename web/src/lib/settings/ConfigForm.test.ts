import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { EditOp } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import ConfigForm from './ConfigForm.svelte';
import { ConfigForm as Form, configPort } from './configForm.svelte.ts';
import { MASKED } from './port.ts';

type Edit = { pointer: string; op: EditOp; valueJson: string };
type Sent = { digest: string; edits: Edit[]; write: boolean };

const CONFIG = 'vibranium.config.yaml';
const file = (value: string) => ({ value, source: CONFIG });

async function rendered(refuse?: ConnectError) {
  const sent: Sent[] = [];
  let digest = 'd1';
  const engine = fakeEngine({
    getProject: () => ({ name: 'shop', profiles: ['dev', 'staging'], defaultProfile: 'dev' }),
    explainConfig: () => ({
      values: [
        { key: 'defaults.timeout', layers: [file('10s')] },
        { key: 'vars.usersUrl', layers: [file('http://localhost:8080')] },
        { key: 'vars.token', layers: [file(MASKED)] },
      ],
      notices: [],
    }),
    readFile: ({ path }) => ({ text: '', digest: path === CONFIG ? digest : '' }),
    editFile: (request) => {
      const edits = request.edits.map(({ pointer, op, valueJson }) => ({ pointer, op, valueJson }));
      sent.push({ digest: request.digest, edits, write: request.write });
      if (refuse) throw refuse;
      if (request.write) digest = 'd2';
      return { diff: `-  timeout: 10s\n+  timeout: 5s\n`, digest, written: request.write };
    },
  });
  const form = new Form(configPort(engine));
  await form.load();
  render(ConfigForm, { form });
  return { sent, form, user: userEvent.setup() };
}

const status = () => screen.getByRole('status').textContent ?? '';

describe('ConfigForm', () => {
  it('shows what the file sets, a secret var masked and read-only', async () => {
    await rendered();
    expect(screen.getByRole('textbox', { name: 'Project' })).toHaveProperty('value', 'shop');
    expect(screen.getByRole('textbox', { name: 'Timeout' })).toHaveProperty('value', '10s');
    expect(screen.getByRole('radio', { name: /dev/ })).toHaveProperty('checked', true);
    expect(screen.getByRole('textbox', { name: /usersUrl/ })).toHaveProperty(
      'value',
      'http://localhost:8080',
    );
    expect(screen.queryByRole('textbox', { name: /token/ })).toBeNull();
    expect(document.body.textContent).toContain(MASKED);
    expect(screen.getByRole('button', { name: 'Save' })).toHaveProperty('disabled', true);
  });

  it('reviews the masked diff without writing, then saves over the digest it read', async () => {
    const { sent, user } = await rendered();
    const timeout = screen.getByRole('textbox', { name: 'Timeout' });
    await user.clear(timeout);
    await user.type(timeout, '5s');
    await user.click(screen.getByRole('radio', { name: /staging/ }));
    expect(status()).toContain('3 unsaved changes.');
    await user.click(screen.getByRole('button', { name: 'Review changes' }));
    expect(screen.getByLabelText('Changes to vibranium.config.yaml').textContent).toContain(
      '+  timeout: 5s',
    );
    await user.click(screen.getByRole('button', { name: 'Save' }));
    const edits: Edit[] = [
      { pointer: '/defaults/timeout', op: EditOp.SET, valueJson: '"5s"' },
      { pointer: '/profiles/0/default', op: EditOp.SET, valueJson: 'false' },
      { pointer: '/profiles/1/default', op: EditOp.SET, valueJson: 'true' },
    ];
    expect(sent).toEqual([
      { digest: 'd1', edits, write: false },
      { digest: 'd1', edits, write: true },
    ]);
    expect(status()).toContain('The project checks clean.');
    expect(screen.queryByLabelText('Changes to vibranium.config.yaml')).toBeNull();
  });

  it('adds and removes profiles and vars, saying why a name will not do', async () => {
    const { form, user } = await rendered();
    await user.type(screen.getByRole('textbox', { name: 'New profile' }), 'Dev{Enter}');
    expect(screen.getByText(/Use a lowercase word/)).toBeTruthy();
    await user.clear(screen.getByRole('textbox', { name: 'New profile' }));
    await user.type(screen.getByRole('textbox', { name: 'New profile' }), 'eu{Enter}');
    await user.click(screen.getByRole('button', { name: 'Remove profile staging' }));
    await user.type(screen.getByRole('textbox', { name: 'New variable' }), 'ordersUrl');
    await user.type(screen.getByRole('textbox', { name: 'Value' }), 'http://x.test{Enter}');
    await user.click(screen.getByRole('button', { name: 'Remove variable token' }));
    expect(form.edits).toEqual([
      { pointer: '/vars/token', op: 'remove' },
      { pointer: '/vars/ordersUrl', op: 'set', value: 'http://x.test' },
      { pointer: '/profiles/1', op: 'remove' },
      { pointer: '/profiles/-', op: 'insert', value: { id: 'eu' } },
    ]);
    await user.click(screen.getByRole('button', { name: 'Discard changes' }));
    expect(form.dirty).toBe(false);
    expect(screen.getByRole('button', { name: 'Remove profile staging' })).toBeTruthy();
  });

  it('offers a reload when the file changed on disk since it was read', async () => {
    const { user } = await rendered(new ConnectError('changed', Code.Aborted));
    await user.clear(screen.getByRole('textbox', { name: 'Project' }));
    await user.type(screen.getByRole('textbox', { name: 'Project' }), 'store');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(screen.getByRole('alert').textContent).toContain('changed on disk');
    await user.click(screen.getByRole('button', { name: 'Reload from disk' }));
    expect(screen.getByRole('textbox', { name: 'Project' })).toHaveProperty('value', 'shop');
  });

  it("shows the engine's refusal and keeps the edits", async () => {
    const refusal = new ConnectError('edit 1 adds an error: V2001', Code.InvalidArgument);
    const { form, user } = await rendered(refusal);
    await user.click(screen.getByRole('button', { name: 'Remove profile dev' }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(screen.getByRole('alert').textContent).toBe('edit 1 adds an error: V2001');
    expect(form.dirty).toBe(true);
  });
});
