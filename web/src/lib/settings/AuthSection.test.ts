import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import AuthSection from './AuthSection.svelte';
import { AuthTest, authTestPort, lifetime } from './authTest.svelte.ts';

type Asked = { auth: string; profiles: readonly string[] };

async function rendered(
  testAuth: (asked: Asked) => object,
  auths = ['svc', 'partner'],
  profile = 'staging',
) {
  const asked: Asked[] = [];
  const engine = fakeEngine({
    getProject: () => ({ auths }),
    testAuth: ({ auth, profiles }) => {
      asked.push({ auth, profiles });
      return testAuth({ auth, profiles });
    },
  });
  const auth = new AuthTest(authTestPort(engine));
  await auth.load();
  render(AuthSection, { auth, profile });
  return { asked, user: userEvent.setup() };
}

const row = (id: string) => screen.getByRole('row', { name: new RegExp(id) });

describe('AuthSection', () => {
  it('lists the config auths, untested until asked', async () => {
    await rendered(() => ({}));
    const table = screen.getByRole('table', { name: 'Auths' });
    expect(
      within(table)
        .getAllByRole('rowheader')
        .map((th) => th.textContent),
    ).toEqual(['svc', 'partner']);
    expect(row('svc').textContent).toContain('Not tested');
  });

  it('tests an auth under the chosen profile and shows the names and expiry only', async () => {
    const { asked, user } = await rendered(() => ({
      api: 'users#me',
      headers: ['authorization'],
      query: [],
      cookies: ['sid'],
      expiresInMs: 3_540_000n,
    }));
    await user.click(screen.getByRole('button', { name: 'Test svc' }));
    expect(asked).toEqual([{ auth: 'svc', profiles: ['staging'] }]);
    const text = row('svc').textContent ?? '';
    for (const shown of ['users#me', 'authorization', 'sid', '59 min', 'Under staging.']) {
      expect(text).toContain(shown);
    }
    expect(row('partner').textContent).toContain('Not tested');
  });

  it('shows the masked reason an auth gave no credentials', async () => {
    const { user } = await rendered(() => ({
      api: 'users#me',
      error: 'V5114: credentials could not be obtained: 401 from ••••',
    }));
    await user.click(screen.getByRole('button', { name: 'Test partner' }));
    expect(row('partner').textContent).toContain('401 from ••••');
  });

  it('shows why a test was refused', async () => {
    const { user } = await rendered(() => {
      throw new ConnectError('no auth `svc`', Code.InvalidArgument);
    });
    await user.click(screen.getByRole('button', { name: 'Test svc' }));
    expect(screen.getByRole('alert').textContent).toContain('no auth `svc`');
    expect(row('svc').textContent).toContain('Not tested');
  });

  it('tests under the default profile when none is picked', async () => {
    const { asked, user } = await rendered(() => ({ api: 'users#me' }), ['svc'], '');
    await user.click(screen.getByRole('button', { name: 'Test svc' }));
    expect(asked).toEqual([{ auth: 'svc', profiles: [] }]);
    expect(row('svc').textContent).toContain('Under the default profile.');
  });

  it('says when the config defines no auth', async () => {
    await rendered(() => ({}), []);
    expect(screen.getByText('The config defines no auth.')).toBeTruthy();
  });

  it('gives a lifetime in its largest whole unit', () => {
    expect([lifetime(500), lifetime(30_000), lifetime(7_200_000), lifetime(90_000_000)]).toEqual([
      '500 ms',
      '30 s',
      '2 h',
      '1 d',
    ]);
  });
});
