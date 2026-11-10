import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import SecretsSection from './SecretsSection.svelte';
import { schemaPort, SecretSchema } from './secrets.svelte.ts';

type Write = { path: string; text: string; expectedDigest?: string };

async function rendered(file?: string, conflict = false) {
  const writes: Write[] = [];
  const engine = fakeEngine({
    readFile: ({ path }) => {
      if (file === undefined) throw new ConnectError('no file', Code.NotFound);
      return { text: path === '.env.schema' ? file : '', digest: 'd1' };
    },
    writeFile: (request) => {
      writes.push({
        path: request.path,
        text: request.text,
        expectedDigest: request.expectedDigest,
      });
      if (conflict) throw new ConnectError('changed', Code.Aborted);
      return { digest: 'd2', diagnostics: [] };
    },
  });
  const schema = new SecretSchema(schemaPort(engine));
  await schema.load();
  render(SecretsSection, { schema });
  return { writes, user: userEvent.setup() };
}

const names = () =>
  within(screen.getByRole('table', { name: 'Declared secrets' }))
    .getAllByRole('rowheader')
    .map((th) => th.textContent);

describe('SecretsSection', () => {
  it('lists the declared names with their annotations, never a value', async () => {
    await rendered('# The checkout token\n# @sensitive\nSHOP_TOKEN=\nHOOK_URL=https://x.test\n');
    expect(names()).toEqual(['SHOP_TOKEN', 'HOOK_URL']);
    const row = screen.getByRole('row', { name: /SHOP_TOKEN/ });
    expect(row.textContent).toContain('@sensitive');
    expect(row.textContent).toContain('The checkout token');
    expect(document.body.textContent).not.toContain('x.test');
  });

  it('adds and removes names, then saves them over the digest it read', async () => {
    const { writes, user } = await rendered('A=\n');
    await user.type(screen.getByRole('textbox', { name: 'New secret name' }), 'PAY_KEY{Enter}');
    await user.click(screen.getByRole('button', { name: 'Remove A' }));
    expect(names()).toEqual(['PAY_KEY']);
    expect(screen.getByRole('status').textContent).toContain('Unsaved changes.');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(writes).toEqual([{ path: '.env.schema', text: 'PAY_KEY=\n', expectedDigest: 'd1' }]);
    expect(screen.getByRole('status').textContent).toContain('The project checks clean.');
    expect(screen.getByRole('button', { name: 'Save' })).toHaveProperty('disabled', true);
  });

  it('refuses a name the engine would not read, and says why', async () => {
    const { user } = await rendered('A=\n');
    const input = screen.getByRole('textbox', { name: 'New secret name' });
    await user.type(input, 'shop-token{Enter}');
    expect(input.getAttribute('aria-invalid')).toBe('true');
    expect(input.getAttribute('aria-describedby')).toBe('secret-name-problem');
    expect(document.getElementById('secret-name-problem')?.textContent).toMatch(
      /not starting with a digit/,
    );
    expect(names()).toEqual(['A']);
  });

  it('creates the file when there is none', async () => {
    const { writes, user } = await rendered();
    expect(screen.getByText('No secrets are declared yet.')).toBeTruthy();
    await user.type(screen.getByRole('textbox', { name: 'New secret name' }), 'A{Enter}');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(writes).toEqual([{ path: '.env.schema', text: 'A=\n', expectedDigest: undefined }]);
  });

  it('offers to reload when the file changed on disk', async () => {
    const { user } = await rendered('A=\n', true);
    await user.click(screen.getByRole('button', { name: 'Remove A' }));
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(screen.getByRole('alert').textContent).toContain('changed on disk');
    await user.click(screen.getByRole('button', { name: 'Reload from disk' }));
    expect(names()).toEqual(['A']);
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
