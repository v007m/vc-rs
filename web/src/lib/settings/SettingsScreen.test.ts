import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { AuthTest, authTestPort } from './authTest.svelte.ts';
import { enginePort, MASKED, type Explained, type SettingsPort } from './port.ts';
import SettingsScreen from './SettingsScreen.svelte';
import { EffectiveSettings } from './settings.svelte.ts';

function explained(profile: string): Explained {
  return {
    values: [
      { key: 'profile', layers: [{ value: profile, source: '--profile' }] },
      {
        key: 'defaults.timeout',
        layers: [
          { value: '1m', source: `profile ${profile}` },
          { value: '30s', source: 'built-in' },
        ],
      },
      { key: 'vars.apiToken', layers: [{ value: MASKED, source: '.env' }] },
    ],
    notices: [],
  };
}

async function rendered() {
  const asked: string[][] = [];
  const port: SettingsPort = {
    profiles: () => Promise.resolve({ profiles: ['dev', 'staging'], defaultProfile: 'staging' }),
    explain: (profiles) => {
      asked.push([...profiles]);
      return Promise.resolve(explained(profiles[0] ?? ''));
    },
  };
  const settings = new EffectiveSettings(port);
  await settings.load();
  render(SettingsScreen, { settings });
  return { asked, user: userEvent.setup() };
}

describe('SettingsScreen', () => {
  it('shows each value, the layer that set it and what it replaced', async () => {
    const { asked } = await rendered();
    expect(asked).toEqual([['staging']]);
    const table = screen.getByRole('table', { name: 'Effective settings' });
    const row = within(table).getByRole('row', { name: /defaults\.timeout/ });
    const cells = within(row).getAllByRole('cell');
    expect(cells[0]?.textContent).toBe('1m');
    expect(cells[1]?.textContent).toContain('profile staging');
    expect(cells[1]?.textContent).toContain('over 30s (built-in)');
  });

  it('shows a secret masked and says so', async () => {
    await rendered();
    const row = screen.getByRole('row', { name: /vars\.apiToken/ });
    expect(row.textContent).toContain(`${MASKED} (secret, masked)`);
  });

  it('explains again under the profile picked', async () => {
    const { asked, user } = await rendered();
    await user.selectOptions(screen.getByRole('combobox', { name: 'Profile' }), 'dev');
    expect(asked).toEqual([['staging'], ['dev']]);
    expect(screen.getByRole('row', { name: /^profile/ }).textContent).toContain('dev');
  });

  it('explains the config through the engine under the default profile', async () => {
    const asked: string[][] = [];
    const settings = new EffectiveSettings(
      enginePort(
        fakeEngine({
          getProject: () => ({ name: 's', profiles: ['dev'], defaultProfile: 'dev' }),
          explainConfig: ({ profiles }) => {
            asked.push(profiles);
            return {
              values: [{ key: 'vars.apiToken', layers: [{ value: MASKED, source: '.env' }] }],
            };
          },
        }),
      ),
    );
    await settings.load();
    render(SettingsScreen, { settings });
    expect(asked).toEqual([['dev']]);
    expect(screen.getByRole('row', { name: /vars\.apiToken/ }).textContent).toContain(
      `${MASKED} (secret, masked)`,
    );
  });

  it('says what went wrong when the engine cannot explain the config', async () => {
    const settings = new EffectiveSettings(
      enginePort(fakeEngine({ getProject: () => ({ name: 's', profiles: ['dev'] }) })),
    );
    await settings.load();
    render(SettingsScreen, { settings });
    expect(screen.getByRole('alert').textContent).not.toBe('');
    expect(screen.getByRole('combobox', { name: 'Profile' })).toHaveProperty('value', 'dev');
  });

  it('tests an auth under the profile picked', async () => {
    const asked: string[][] = [];
    const engine = fakeEngine({
      getProject: () => ({ name: 's', profiles: ['dev', 'staging'], auths: ['svc'] }),
      explainConfig: () => ({ values: [] }),
      testAuth: ({ profiles }) => {
        asked.push(profiles);
        return { api: 'users#me', headers: ['authorization'] };
      },
    });
    const settings = new EffectiveSettings(enginePort(engine));
    const auth = new AuthTest(authTestPort(engine));
    await Promise.all([settings.load(), auth.load()]);
    render(SettingsScreen, { settings, auth });
    const user = userEvent.setup();
    await user.selectOptions(screen.getByRole('combobox', { name: 'Profile' }), 'staging');
    await user.click(screen.getByRole('button', { name: 'Test svc' }));
    expect(asked).toEqual([['staging']]);
    expect(screen.getByRole('row', { name: /svc/ }).textContent).toContain('authorization');
  });
});
