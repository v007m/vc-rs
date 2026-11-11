import { ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { RunSession } from '#lib/run/session.svelte.ts';
import { enginePort as skipPort } from '#lib/skip/port.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { Catalog } from './catalog.svelte.ts';
import Explorer from './Explorer.svelte';

const definitions = [
  {
    id: 'users.crud#createsUser',
    name: 'Creates a user',
    path: 'services/users/crud.test.yaml',
    tags: ['smoke'],
  },
  {
    id: 'users.crud#rejectsBadEmail',
    name: 'Rejects a bad email',
    path: 'services/users/crud.test.yaml',
    skipped: true,
  },
  {
    id: 'orders.place#placesOrder',
    name: 'Places an order',
    path: 'services/orders/place.test.yaml',
    tags: ['smoke'],
    span: { file: 'services/orders/place.test.yaml', startLine: 12 },
    steps: 2,
    calls: [
      { api: 'orders#create', method: 'POST', path: '/orders' },
      { api: 'orders#get', method: 'GET', path: '/orders/{orderId}' },
    ],
  },
];

const CRUD = `tests:
  - id: createsUser
    name: Creates a user
    steps:
      - call: users#create
`;
const SKIPPED = `${CRUD}    skip: flaky upstream
`;

/** The Skip and Unskip calls the engine was sent, as `[rpc, target, reason, write]`. */
type SkipCall = [string, string, string, boolean];

async function setup(
  runs: string[][] = [],
  asked: boolean[] = [],
  sinces: (string | undefined)[] = [],
  skips: SkipCall[] = [],
) {
  const path = 'services/users/crud.test.yaml';
  const engine = fakeEngine({
    skip: ({ target, reason, write }) => {
      skips.push(['skip', target, reason, write]);
      return { files: [{ path, before: CRUD, after: SKIPPED }], titles: [`Skip ${target}`] };
    },
    unskip: ({ target, write }) => {
      skips.push(['unskip', target, '', write]);
      return { files: [{ path, before: SKIPPED, after: CRUD }], titles: [`Unskip ${target}`] };
    },
    listDefinitions: (request) => {
      const since = request.selection?.changed;
      if (since) sinces.push(since);
      if (since === 'bad') throw new ConnectError('unknown revision bad');
      if (since) return { definitions: definitions.filter((d) => d.id.startsWith('orders.')) };
      asked.push(request.skipped);
      return { definitions };
    },
    // eslint-disable-next-line require-yield -- the run ends without events
    async *run(request) {
      runs.push(request.selection?.ids ?? []);
    },
  });
  const catalog = new Catalog(engine);
  await catalog.load();
  render(Explorer, { catalog, session: new RunSession(engine), skips: skipPort(engine) });
  return { tree: screen.getByRole('tree', { name: 'Tests' }), user: userEvent.setup() };
}

describe('Explorer', () => {
  it('lists the tests as a tree of files, skipped ones included', async () => {
    const asked: boolean[] = [];
    const { tree } = await setup([], asked);
    // `skipped: true` would list only the skipped tests.
    expect(asked).toEqual([false]);
    expect(screen.getByText('3 tests in 2 files')).toBeTruthy();
    expect(within(tree).getAllByRole('treeitem')).toHaveLength(5);
  });

  it('moves through the tree with the arrow keys and closes a file', async () => {
    const { tree, user } = await setup();
    const items = within(tree).getAllByRole('treeitem');
    expect(items[0]?.tabIndex).toBe(0);
    items[0]?.focus();
    await user.keyboard('{ArrowDown}');
    expect(document.activeElement).toBe(items[1]);
    await user.keyboard('{ArrowLeft}{ArrowLeft}');
    expect(items[0]?.getAttribute('aria-expanded')).toBe('false');
    expect(within(tree).getAllByRole('treeitem')).toHaveLength(3);
  });

  it('filters with a facet and shows an empty state', async () => {
    const { tree, user } = await setup();
    await user.click(screen.getByRole('button', { name: /orders/ }));
    expect(
      screen.getByRole('searchbox', { name: 'Filter tests with query syntax' }),
    ).toHaveProperty('value', 'service:orders');
    expect(within(tree).getAllByRole('treeitem')).toHaveLength(2);
    await user.type(screen.getByRole('searchbox'), ' nothing');
    expect(screen.getByText('No test matches “service:orders nothing”.')).toBeTruthy();
  });

  it('asks the engine which tests changed since a git ref', async () => {
    const asked: (string | undefined)[] = [];
    const { user } = await setup([], [], asked);
    await user.type(screen.getByRole('searchbox'), 'changed:main');
    expect(screen.getByRole('status').textContent).toBe('Finding the tests changed since main…');
    expect(await screen.findByText('Places an order')).toBeTruthy();
    const tree = screen.getByRole('tree', { name: 'Tests' });
    expect(within(tree).getAllByRole('treeitem')).toHaveLength(2);
    expect(asked).toEqual(['main']);
    await user.clear(screen.getByRole('searchbox'));
    await user.type(screen.getByRole('searchbox'), 'changed:bad');
    expect((await screen.findByRole('alert')).textContent).toBe(
      'Could not find the changed tests: bad: unknown revision bad',
    );
  });

  it('shows a failure only for the refs in the query', async () => {
    const { user } = await setup();
    await user.type(screen.getByRole('searchbox'), 'changed:bad');
    expect((await screen.findByRole('alert')).textContent).toBe(
      'Could not find the changed tests: bad: unknown revision bad',
    );
    await user.clear(screen.getByRole('searchbox'));
    await user.type(screen.getByRole('searchbox'), 'changed:main');
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByRole('status').textContent).toBe('Finding the tests changed since main…');
  });

  it('selects tests with Space and runs the selection', async () => {
    const runs: string[][] = [];
    const { tree, user } = await setup(runs);
    within(tree).getAllByRole('treeitem')[0]?.focus();
    await user.keyboard('{ArrowDown} {ArrowDown} ');
    expect(screen.getByText('2 selected')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Run selected' }));
    expect(runs).toEqual([['users.crud#createsUser', 'users.crud#rejectsBadEmail']]);
  });

  it('opens a test in the detail panel and runs it', async () => {
    const runs: string[][] = [];
    const { user } = await setup(runs);
    await user.click(screen.getByText('Places an order'));
    expect(screen.getByRole('heading', { name: 'Places an order' })).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Open in editor' }).getAttribute('href')).toContain(
      'file=services%2Forders%2Fplace.test.yaml',
    );
    await user.click(screen.getByRole('button', { name: 'Run this test' }));
    expect(runs).toEqual([['orders.place#placesOrder']]);
  });

  it('previews a skip with its reason, writes it, then reads the tests again', async () => {
    const asked: boolean[] = [];
    const skips: SkipCall[] = [];
    const { user } = await setup([], asked, [], skips);
    await user.click(screen.getByText('Creates a user'));
    await user.click(screen.getByRole('button', { name: 'Skip…' }));
    const form = screen.getByRole('group', { name: 'Skip users.crud#createsUser' });
    const preview = within(form).getByRole('button', { name: 'Preview' });
    expect(preview.hasAttribute('disabled')).toBe(true);
    await user.type(within(form).getByRole('textbox', { name: 'Reason' }), ' flaky upstream ');
    await user.click(preview);
    const diff = within(form).getByRole('region', {
      name: `Changes to services/users/crud.test.yaml`,
    });
    expect(diff.textContent).toContain('skip: flaky upstream');
    await user.click(within(form).getByRole('button', { name: 'Write skip' }));
    expect(skips).toEqual([
      ['skip', 'users.crud#createsUser', 'flaky upstream', false],
      ['skip', 'users.crud#createsUser', 'flaky upstream', true],
    ]);
    expect(screen.queryByRole('group', { name: /^Skip / })).toBeNull();
    expect(asked).toEqual([false, false]);
  });

  it('removes the skip of a skipped test after a preview', async () => {
    const skips: SkipCall[] = [];
    const { user } = await setup([], [], [], skips);
    await user.click(screen.getByText('Rejects a bad email'));
    expect(screen.getByText(/This test is skipped/)).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Unskip…' }));
    const form = screen.getByRole('group', { name: 'Unskip users.crud#rejectsBadEmail' });
    expect(within(form).queryByRole('textbox')).toBeNull();
    const remove = within(form).getByRole('button', { name: 'Remove skip' });
    expect(remove.hasAttribute('disabled')).toBe(true);
    await user.click(within(form).getByRole('button', { name: 'Preview' }));
    await user.click(remove);
    expect(skips.map(([rpc, target, , write]) => [rpc, target, write])).toEqual([
      ['unskip', 'users.crud#rejectsBadEmail', false],
      ['unskip', 'users.crud#rejectsBadEmail', true],
    ]);
  });

  it('says why the engine refused a skip, and writes nothing', async () => {
    const engine = fakeEngine({
      listDefinitions: () => ({ definitions }),
      skip: () => {
        throw new ConnectError('`users.crud#createsUser` is already skipped');
      },
    });
    const catalog = new Catalog(engine);
    await catalog.load();
    render(Explorer, { catalog, session: new RunSession(engine), skips: skipPort(engine) });
    const user = userEvent.setup();
    await user.click(screen.getByText('Creates a user'));
    await user.click(screen.getByRole('button', { name: 'Skip…' }));
    await user.type(screen.getByRole('textbox', { name: 'Reason' }), 'flaky');
    await user.click(screen.getByRole('button', { name: 'Preview' }));
    expect(screen.getByRole('alert').textContent).toContain('is already skipped');
    expect(screen.getByRole('button', { name: 'Write skip' }).hasAttribute('disabled')).toBe(true);
  });

  it('shows where a test is defined, its steps and the apis it calls', async () => {
    const { user } = await setup();
    await user.click(screen.getByText('Places an order'));
    expect(screen.getByText('services/orders/place.test.yaml:12')).toBeTruthy();
    expect(screen.getByText('A flow of 2 steps')).toBeTruthy();
    const calls = within(screen.getByRole('list', { name: 'Calls' })).getAllByRole('listitem');
    expect(calls.map((li) => li.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
      'POST /orders orders#create',
      'GET /orders/{orderId} orders#get',
    ]);
  });
});
