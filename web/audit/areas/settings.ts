import type { MessageInitShape } from '@bufbuild/protobuf';
import type { Page } from '@playwright/test';
import type * as E from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { click } from '../act.ts';
import { Refused, type Fakes } from '../engine.ts';
import { configPath, configYaml, crudYaml } from '../fixtures.ts';
import type { State } from '../states.ts';

const userPath = '~/.config/vibranium/config.yaml';

const userBefore = `profile: staging
defaults:
  timeout: 12s
  retry:
    attempts: 2
reports:
  dir: ~/vc-reports
vars:
  apiToken: ••••
`;

const userAfter = userBefore.replace('timeout: 12s', 'timeout: 30s');

const alertsYaml = `${configYaml}alerts:
  - on: failure
    slack: { webhook: "{{secrets.SLACK_HOOK}}" }
  - on: [failure, recovery]
    profiles: [prod]
    webhook: { url: https://hooks.shop.dev/vc }
`;

const reporter = {
  id: 'htmlReport',
  version: '2.1.0',
  kind: 'reporter',
  command: 'node',
  args: ['dist/index.js'],
  permissions: { network: [], secrets: [], files: ['.vc/reports/**'] },
};

const plugins: MessageInitShape<typeof E.PluginListingSchema>[] = [
  {
    id: 'htmlReport',
    source: 'npm:@acme/vibranium-report@^2',
    manifest: reporter,
    granted: { files: ['.vc/reports/**'] },
    state: 'ready',
    detail: 'ready',
  },
  {
    id: 'shopAuth',
    source: 'path:plugins/shop-auth',
    manifest: {
      id: 'shopAuth',
      version: '0.3.0',
      kind: 'auth',
      command: 'node',
      permissions: { network: ['auth.shop.dev'], secrets: ['SHOP_CLIENT_SECRET'], files: [] },
    },
    granted: {},
    state: 'notApproved',
    detail: 'not approved yet: run vc plugin add path:plugins/shop-auth to approve it',
  },
];

const lockBefore = `plugins:
  htmlReport:
    source: npm:@acme/vibranium-report@^2
`;

/** The engine answers the settings screens need (batch E). */
export const fakes: Fakes = {
  readUserSettings: () => ({
    path: userPath,
    digest: 'u1',
    settings: [
      { key: 'profile', value: 'staging' },
      { key: 'defaults.timeout', value: '12s', shadowed: true },
      { key: 'defaults.retry.attempts', value: '2' },
      { key: 'reports.dir', value: '~/vc-reports' },
      { key: 'vars.apiToken', value: '••••' },
    ],
  }),
  writeUserSettings: () => ({ path: userPath, before: userBefore, after: userAfter, digest: 'u1' }),
  testAlert: () => ({
    sent: [
      { rule: 1, channel: 'slack' },
      { rule: 2, channel: 'webhook', error: 'the webhook answered 502 Bad Gateway' },
    ],
  }),
  pluginList: () => ({ plugins }),
  pluginAdd: () => ({
    manifest: reporter,
    resolved: '@acme/vibranium-report@2.1.0',
    integrity: 'sha512-9f2c1e0b7a64d3',
    warnings: ['htmlReport is not in reports.formats, so no run uses it yet.'],
    files: [
      {
        path: configPath,
        before: configYaml,
        after: `${configYaml}plugins:\n  - npm:@acme/vibranium-report@^2\n`,
      },
      { path: 'vibranium.lock', after: lockBefore },
    ],
    titles: ['Add the plugin htmlReport'],
    digest: 'p1',
  }),
  pluginRemove: () => ({
    files: [{ path: 'vibranium.lock', before: lockBefore, after: 'plugins: {}\n' }],
    titles: ['Remove the plugin shopAuth'],
    digest: 'p2',
  }),
};

/** The config with alert rules, which the Alerts page reads as text. */
const withAlerts: Fakes = {
  readFile: ({ path }) => ({ text: path === configPath ? alertsYaml : crudYaml, digest: 'd1' }),
};

/** Opens the add dialog and previews an npm source. */
async function previewAdd(page: Page): Promise<void> {
  await click(page, 'Add a plugin');
  await page.getByLabel('Source').fill('npm:@acme/vibranium-report@^2');
  await click(page, 'Preview');
  await page.getByRole('region', { name: 'Preview of the plugin' }).waitFor();
}

/** The settings screens and overlays the audit checks, a refused plugin source among them. */
export const states: State[] = [
  { name: 'settings-user', path: '/settings/user' },
  {
    name: 'settings-user-preview',
    path: '/settings/user',
    act: async (page) => {
      await click(page, 'Edit defaults.timeout');
      await page.getByLabel('Value').fill('30s');
      await click(page, 'Set');
      await click(page, 'Preview');
      await page.getByRole('region', { name: 'Preview of your settings' }).waitFor();
    },
  },
  { name: 'settings-alerts', path: '/settings/alerts', fakes: withAlerts },
  {
    name: 'settings-alerts-sent',
    path: '/settings/alerts',
    fakes: withAlerts,
    act: async (page) => {
      await click(page, 'Send a test alert');
      await page.getByRole('table', { name: 'Test alert results' }).waitFor();
    },
  },
  { name: 'settings-plugins', path: '/settings/plugins' },
  { name: 'settings-plugins-add', path: '/settings/plugins', act: previewAdd },
  {
    name: 'settings-plugins-refused',
    path: '/settings/plugins',
    fakes: {
      pluginAdd: () => {
        throw new Refused('that is not a plugin source');
      },
    },
    act: async (page) => {
      await click(page, 'Add a plugin');
      await page.getByLabel('Source').fill('git+https://example.com/report.git');
      await click(page, 'Preview');
      await page.getByText('that is not a plugin source').waitFor();
    },
  },
];
