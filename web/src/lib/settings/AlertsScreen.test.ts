import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { Toasts } from '#lib/ui/toasts.svelte.ts';
import { Alerts, alertsPort, sentText } from './alerts.svelte.ts';
import { alertRules } from './alertsRules.ts';
import AlertsScreen from './AlertsScreen.svelte';

const config = [
  'project: shop',
  'alerts:',
  '  - on: failure',
  '    slack: { webhook: "{{secrets.SLACK_HOOK}}" }',
  '  - on: [failure, recovery]',
  '    profiles: [qa]',
  '    webhook: { url: https://hooks.example.com/vc }',
  'reports:',
  '  dir: .vc/reports',
].join('\n');

describe('alertRules', () => {
  it('splits a block list at its items, dedented', () => {
    expect(alertRules(config)).toEqual([
      '- on: failure\n  slack: { webhook: "{{secrets.SLACK_HOOK}}" }',
      '- on: [failure, recovery]\n  profiles: [qa]\n  webhook: { url: https://hooks.example.com/vc }',
    ]);
  });

  it('is nothing with no alerts or an empty list, and one text for a flow list', () => {
    expect(alertRules('project: shop\n')).toEqual([]);
    expect(alertRules('alerts: []  # none yet\n')).toEqual([]);
    expect(alertRules('alerts: [{ on: failure, slack: {} }]\n')).toEqual([
      '[{ on: failure, slack: {} }]',
    ]);
  });
});

describe('sentText', () => {
  it('counts the channels that took it and those that did not', () => {
    expect(sentText([{ rule: 1, channel: 'slack' }])).toBe('Sent 1 test alert.');
    expect(
      sentText([
        { rule: 1, channel: 'slack' },
        { rule: 2, channel: 'webhook', error: 'x' },
      ]),
    ).toBe('Sent 1 test alert. 1 failed.');
  });
});

async function rendered(profile: string | undefined, fail?: ConnectError) {
  const asked: string[][] = [];
  const client = fakeEngine({
    readFile: ({ path }) => ({ text: path === 'vibranium.config.yaml' ? config : '', digest: 'd' }),
    testAlert: ({ profiles }) => {
      asked.push(profiles);
      if (fail) throw fail;
      return {
        sent: [
          { rule: 1, channel: 'slack' },
          { rule: 2, channel: 'webhook', error: 'the webhook answered 500' },
        ],
      };
    },
  });
  const alerts = new Alerts(alertsPort(client));
  await alerts.load();
  const toasts = new Toasts(() => undefined);
  render(AlertsScreen, { alerts, profile, toasts });
  return { asked, toasts, user: userEvent.setup() };
}

describe('AlertsScreen', () => {
  it('shows the rules read only, with a way to the editor', async () => {
    await rendered('qa');
    const rules = screen.getByRole('table', { name: 'Alert rules' });
    expect(rules.textContent).toContain('Rule 1');
    expect(rules.textContent).toContain('profiles: [qa]');
    expect(
      screen.getByRole('link', { name: 'Open it in the editor' }).getAttribute('href'),
    ).toMatch(/\/tests\/editor\?file=vibranium\.config\.yaml$/);
    expect(screen.queryByRole('textbox')).toBeNull();
  });

  it("sends a test alert under the shell's profile and shows each channel", async () => {
    const { user, asked, toasts } = await rendered('qa');
    await user.click(screen.getByRole('button', { name: 'Send a test alert' }));
    const results = await screen.findByRole('table', { name: 'Test alert results' });
    expect(asked).toEqual([['qa']]);
    const rows = results.querySelectorAll('tbody tr');
    expect(rows[0]?.textContent).toContain('slack');
    expect(rows[0]?.querySelector('.st-dot')?.classList.contains('vc-pass')).toBe(true);
    expect(rows[1]?.textContent).toContain('Failed: the webhook answered 500');
    expect(rows[1]?.querySelector('.st-dot')?.classList.contains('vc-fail')).toBe(true);
    expect(toasts.list.map((toast) => [toast.text, toast.tone])).toEqual([
      ['Sent 1 test alert. 1 failed.', 'done'],
    ]);
  });

  it('sends no profile for the default one, and toasts a refusal', async () => {
    const refusal = new ConnectError('the config names no alerts', Code.FailedPrecondition);
    const { user, asked, toasts } = await rendered(undefined, refusal);
    expect(screen.getByText('the default profile')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Send a test alert' }));
    expect((await screen.findByRole('alert')).textContent).toBe('the config names no alerts');
    expect(asked).toEqual([[]]);
    expect(toasts.list.map((toast) => toast.tone)).toEqual(['error']);
  });

  it('says so on an engine without TestAlert', async () => {
    const alerts = new Alerts(alertsPort(fakeEngine({ readFile: () => ({ text: config }) })));
    await alerts.load();
    render(AlertsScreen, { alerts, profile: 'qa' });
    await userEvent.setup().click(screen.getByRole('button', { name: 'Send a test alert' }));
    expect(await screen.findByText('Test alerts are not available')).toBeTruthy();
  });
});
