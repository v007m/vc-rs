import type { MessageInitShape } from '@bufbuild/protobuf';
import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { beforeEach, describe, expect, it } from 'vitest';
import type {
  PluginAddRequestSchema,
  PluginListingSchema,
  PluginRemoveRequestSchema,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { stubResizeObserver } from '#lib/testing/layout.ts';
import { Toasts } from '#lib/ui/toasts.svelte.ts';
import { Plugins, pluginsPort } from './plugins.svelte.ts';
import PluginsScreen from './PluginsScreen.svelte';

type Listing = MessageInitShape<typeof PluginListingSchema>;
type Add = MessageInitShape<typeof PluginAddRequestSchema>;
type Remove = MessageInitShape<typeof PluginRemoveRequestSchema>;

const manifest = {
  id: 'htmlReport',
  version: '2.1.0',
  kind: 'reporter',
  permissions: { network: [], secrets: [], files: ['.vc/reports/**'] },
};

const listed: Listing[] = [
  {
    id: 'htmlReport',
    source: 'npm:@acme/vibranium-report@^2',
    manifest,
    granted: { files: ['.vc/reports/**'] },
    state: 'ready',
    detail: 'ready',
  },
  {
    id: 'shopAuth',
    source: 'path:plugins/shop-auth',
    state: 'notApproved',
    detail: 'not approved: run vc plugin add path:plugins/shop-auth',
  },
];

const lock = {
  path: 'vibranium.lock',
  before: 'plugins: {}\n',
  after: 'plugins:\n  htmlReport: {}\n',
};

function engine(plugins: Listing[], refuse?: ConnectError) {
  const adds: Add[] = [];
  const removes: Remove[] = [];
  const client = fakeEngine({
    pluginList: () => ({ plugins }),
    pluginAdd: (request) => {
      adds.push({ ...request });
      if (refuse) throw refuse;
      return {
        manifest,
        resolved: '@acme/vibranium-report@2.1.0',
        integrity: 'sha512-abc',
        warnings: ['htmlReport is not in reports.formats yet'],
        files: [lock],
        digest: 'a1',
      };
    },
    pluginRemove: (request) => {
      removes.push({ ...request });
      return { files: [lock], digest: 'r1' };
    },
  });
  return { client, adds, removes };
}

async function rendered(plugins: Listing[] = listed, refuse?: ConnectError) {
  const fake = engine(plugins, refuse);
  const state = new Plugins(pluginsPort(fake.client));
  await state.load();
  const toasts = new Toasts(() => undefined);
  render(PluginsScreen, { plugins: state, toasts });
  return { ...fake, toasts, user: userEvent.setup() };
}

beforeEach(() => stubResizeObserver());

describe('PluginsScreen', () => {
  it('lists each plugin with its source, reach, grant and state', async () => {
    await rendered();
    const ready = screen.getByRole('row', { name: /htmlReport/ });
    expect(ready.textContent).toContain('2.1.0 · reporter');
    expect(ready.textContent).toContain('npm:@acme/vibranium-report@^2');
    expect(within(ready).getAllByText('files .vc/reports/**')).toHaveLength(2);
    expect(within(ready).getByText('Ready').classList.contains('vc-pass')).toBe(true);
    const waiting = screen.getByRole('row', { name: /shopAuth/ });
    expect(waiting.textContent).toContain('Nothing yet');
    expect(within(waiting).getByText('Not approved').classList.contains('vc-blocked')).toBe(true);
    expect(waiting.textContent).toContain(
      'Not approved yet. Run vc plugin add path:plugins/shop-auth to approve it.',
    );
    expect(within(waiting).getByText('vc plugin add path:plugins/shop-auth').tagName).toBe('CODE');
  });

  it('previews an add, then adds with the digest and lists again', async () => {
    const { user, adds, toasts } = await rendered();
    await user.click(screen.getByRole('button', { name: 'Add a plugin' }));
    const dialog = screen.getByRole('dialog', { name: 'Add a plugin' });
    await user.type(
      within(dialog).getByRole('textbox', { name: 'Source' }),
      'npm:@acme/vibranium-report',
    );
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    const offer = await within(dialog).findByRole('region', { name: 'Preview of the plugin' });
    expect(offer.textContent).toContain('@acme/vibranium-report@2.1.0');
    expect(offer.textContent).toContain('sha512-abc');
    expect(within(offer).getByText('files .vc/reports/**')).toBeTruthy();
    expect(within(offer).getByText('htmlReport is not in reports.formats yet').className).toContain(
      'vc-banner',
    );
    const buttons = within(dialog)
      .getAllByRole('button')
      .map((button) => button.textContent);
    expect(buttons.slice(-2)).toEqual(['Cancel', 'Add']);
    await user.click(within(dialog).getByRole('button', { name: 'Add' }));
    expect(adds).toEqual([
      expect.objectContaining({ source: 'npm:@acme/vibranium-report', write: false }),
      expect.objectContaining({
        source: 'npm:@acme/vibranium-report',
        write: true,
        expectedDigest: 'a1',
      }),
    ]);
    expect(toasts.list.map((toast) => toast.text)).toEqual(['Added the plugin htmlReport.']);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('shows a refused source under the field, with the format hint', async () => {
    const refusal = new ConnectError('that is not a plugin source', Code.InvalidArgument);
    const { user } = await rendered(listed, refusal);
    await user.click(screen.getByRole('button', { name: 'Add a plugin' }));
    const dialog = screen.getByRole('dialog', { name: 'Add a plugin' });
    const field = within(dialog).getByRole('textbox', { name: 'Source' });
    await user.type(field, 'git+https://example.com/p.git');
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    expect((await within(dialog).findByRole('alert')).textContent).toBe(
      'that is not a plugin source',
    );
    expect(field.getAttribute('aria-invalid')).toBe('true');
    expect(dialog.textContent).toContain('npm:<package>[@<range>]');
    expect(within(dialog).queryByRole('region', { name: 'Preview of the plugin' })).toBeNull();
  });

  it('previews a removal, then removes with the digest', async () => {
    const { user, removes, toasts } = await rendered();
    await user.click(screen.getByRole('button', { name: 'Remove shopAuth' }));
    const dialog = await screen.findByRole('dialog', { name: 'Remove shopAuth?' });
    expect(await within(dialog).findByRole('heading', { name: 'vibranium.lock' })).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
    expect(removes).toEqual([
      expect.objectContaining({ id: 'shopAuth', write: false }),
      expect.objectContaining({ id: 'shopAuth', write: true, expectedDigest: 'r1' }),
    ]);
    expect(toasts.list.map((toast) => toast.text)).toEqual(['Removed the plugin shopAuth.']);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('floats an empty state with a way to add the first plugin', async () => {
    const { user } = await rendered([]);
    expect(screen.getByText('No plugins yet')).toBeTruthy();
    expect(screen.queryByRole('table')).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Add a plugin' }));
    expect(screen.getByRole('dialog', { name: 'Add a plugin' })).toBeTruthy();
  });
});
