import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { beforeAll, describe, expect, it } from 'vitest';
import { RunSession } from '#lib/run/session.svelte.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { stubResizeObserver } from '#lib/testing/layout.ts';
import SlotHost from '#lib/testing/SlotHost.svelte';
import { fakeStream } from '#lib/testing/stream.ts';
import ClientScreen from './ClientScreen.svelte';
import { ClientLayout } from './layout.svelte.ts';
import type { CallOutput, ClientPort } from './port.ts';
import { available, noSeams, type AdhocRequest, type ClientSeams } from './seams.ts';
import { ClientSession } from './session.svelte.ts';

const body = { id: 'u_1', email: 'grace@example.com', tags: ['a', 'b'] };

function output(status: number, text: string): CallOutput {
  const responseJson = JSON.stringify({
    status,
    headers: { 'content-type': 'application/json' },
    body,
  });
  return { exit: 0, text, notices: [], responseJson, checks: [], saved: [] };
}

const port: ClientPort = {
  apis: () =>
    Promise.resolve([{ id: 'users#getUser', name: 'Get a user', path: 'u.api.yaml', vars: [] }]),
  flows: () => Promise.resolve([]),
  call: () => Promise.resolve(output(200, '200 OK · 4 ms')),
  readDefinition: (id) =>
    Promise.resolve({
      path: 'u.api.yaml',
      pointer: '/apis/0',
      digest: 'd1',
      node: { id: id.split('#')[1], method: 'GET', path: '/users/u_1' },
    }),
  explainApi: () => Promise.resolve({ vars: [], headers: [], curl: '', notices: [] }),
  editFile: () => Promise.reject(new Error('unused')),
  saveCallAsTest: () => Promise.reject(new Error('unused')),
  importHar: () => Promise.reject(new Error('unused')),
};

async function rendered(seams: Partial<ClientSeams> = {}) {
  const all: ClientSeams = { ...noSeams, ...seams };
  const client = new ClientSession(port, all);
  await client.load();
  const layout = new ClientLayout(undefined);
  const session = new RunSession(fakeEngine({}));
  const args = { client, port, session, layout, history: all };
  render(SlotHost<typeof args>, { screen: ClientScreen, args });
  return { client, layout, user: userEvent.setup() };
}

/** The top row's tab strip of open requests. */
function strip(): HTMLElement {
  return screen.getByRole('navigation', { name: 'Open requests' });
}

async function newTab(user: ReturnType<typeof userEvent.setup>): Promise<void> {
  await user.click(within(strip()).getByRole('button', { name: 'New request' }));
}

beforeAll(stubResizeObserver);

describe('Client space', () => {
  it('sends a scratch request through the ad hoc seam, and closes its tab', async () => {
    const asked: AdhocRequest[] = [];
    const { user } = await rendered({
      callAdhoc: (request) => {
        asked.push(request);
        return Promise.resolve(available(output(201, '201 Created · 3 ms')));
      },
    });
    await newTab(user);
    expect(within(strip()).getByRole('button', { name: 'GET Untitled request' })).toBeTruthy();
    await user.type(screen.getByRole('textbox', { name: 'URL' }), 'http://localhost:7421/users');
    await user.click(screen.getByRole('button', { name: 'Send' }));
    expect(asked.map((r) => [r.type, r.method, r.url])).toEqual([
      ['http', 'GET', 'http://localhost:7421/users'],
    ]);
    const response = screen.getByRole('region', { name: 'Response' });
    expect(within(response).getByText('Created · 3 ms')).toBeTruthy();
    expect(within(response).getByRole('list', { name: 'Response body' })).toBeTruthy();
    await user.click(within(strip()).getByRole('button', { name: 'Close Untitled request' }));
    expect(within(strip()).queryAllByRole('listitem')).toEqual([]);
  });

  it('adds a check from a value in the body, and filters the body with a JSONPath', async () => {
    const { user, client } = await rendered();
    await user.click(screen.getByRole('treeitem', { name: 'Get a user' }));
    await user.keyboard('{Meta>}{Enter}{/Meta}');
    const response = screen.getByRole('region', { name: 'Response' });
    await user.click(within(response).getByRole('button', { name: '$.id = "u_1". Add a check' }));
    const pick = within(response).getByRole('group', { name: 'Add a check on $.id' });
    await user.click(within(pick).getByRole('button', { name: 'Equals' }));
    expect(client.request?.checks).toEqual([
      { kind: 'body', path: '$.id', op: 'equals', value: '"u_1"' },
    ]);
    expect(within(response).queryByRole('group', { name: 'Add a check on $.id' })).toBeNull();
    expect(document.activeElement?.getAttribute('data-path')).toBe('$.id');
    await user.type(
      within(response).getByRole('searchbox', { name: 'Filter the body with a JSONPath' }),
      '$.tags[[*]',
    );
    expect(within(response).getByRole('status').textContent?.replace(/\s+/g, ' ')).toContain(
      '2 matches for $.tags[*]',
    );
    await user.click(screen.getByRole('tab', { name: /^Expect/ }));
    expect(screen.getByRole('list', { name: 'Added checks' }).textContent).toContain('$.id');
  });

  it('connects a WebSocket, shows its messages, sends from the composer and disconnects', async () => {
    const fake = fakeStream();
    const { user } = await rendered(fake.port);
    await newTab(user);
    await user.selectOptions(screen.getByRole('combobox', { name: 'Request type' }), 'websocket');
    expect(within(strip()).getByRole('button', { name: 'WS Untitled request' })).toBeTruthy();
    expect(screen.queryByRole('tab', { name: 'Body' })).toBeNull();
    await user.type(screen.getByRole('textbox', { name: 'URL' }), 'ws://localhost:7421/chat');
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    fake.push({ kind: 'opened', stream: 's1', runId: 1 });
    fake.push({ kind: 'message', direction: 'in', atMs: 1, json: '{"text":"welcome"}' });
    const list = await screen.findByRole('list', { name: 'Stream messages' });
    await within(list).findByText('welcome');
    expect(within(list).getByText('text')).toBeTruthy();
    await user.type(screen.getByRole('textbox', { name: 'Message' }), 'hello');
    await user.click(screen.getByRole('button', { name: 'Send message' }));
    expect(fake.sent).toEqual([['s1', 'hello']]);
    await within(list).findByText(/Sent/);
    await user.click(screen.getByRole('button', { name: 'Disconnect' }));
    expect(await screen.findByText(/Ended: the stream closed/)).toBeTruthy();
    expect(fake.sent.at(-1)).toEqual(['s1', undefined]);
  });

  it('only listens on SSE: no direction filter and no composer, and pauses to hold messages', async () => {
    const fake = fakeStream();
    const { user } = await rendered(fake.port);
    await newTab(user);
    await user.selectOptions(screen.getByRole('combobox', { name: 'Request type' }), 'sse');
    await user.type(screen.getByRole('textbox', { name: 'URL' }), 'http://localhost:7421/events');
    await user.click(screen.getByRole('button', { name: 'Connect' }));
    fake.push({ kind: 'opened', stream: 's2', runId: 2 });
    const messages = screen.getByRole('region', { name: 'Messages' });
    expect(within(messages).queryByRole('group', { name: 'Direction' })).toBeNull();
    expect(within(messages).queryByRole('textbox', { name: 'Message' })).toBeNull();
    await user.click(within(messages).getByRole('button', { name: 'Pause' }));
    fake.push({ kind: 'message', direction: 'in', atMs: 1, json: '{"event":"tick","data":"1"}' });
    expect(await within(messages).findByText('1 held')).toBeTruthy();
    await user.click(within(messages).getByRole('button', { name: 'Resume' }));
    expect(within(messages).getByText('tick')).toBeTruthy();
  });

  it('lists recent calls by day and reopens one as a scratch tab', async () => {
    const { user } = await rendered({
      recent: () =>
        Promise.resolve(
          available([
            {
              id: 7,
              method: 'POST',
              label: 'http://localhost:7421/users',
              status: 201,
              outcome: 'passed',
              startedMs: Date.now(),
            },
          ]),
        ),
    });
    const today = await screen.findByRole('list', { name: 'Calls Today' });
    await user.click(within(today).getByRole('button', { name: /localhost:7421\/users/ }));
    expect(within(strip()).getByRole('button', { name: /^POST/ })).toBeTruthy();
    expect(screen.getByRole<HTMLInputElement>('textbox', { name: 'URL' }).value).toBe(
      'http://localhost:7421/users',
    );
  });

  it('says so when the engine keeps no history', async () => {
    await rendered();
    expect(await screen.findByText('Call history needs a newer engine.')).toBeTruthy();
  });

  it('resizes the cards from the keyboard and remembers the split', async () => {
    const { user, layout } = await rendered();
    const divider = screen.getByRole('separator', { name: 'Resize request and response' });
    divider.focus();
    await user.keyboard('{ArrowRight}');
    expect(divider.getAttribute('aria-valuenow')).toBe('52');
    await user.keyboard('{Shift>}{ArrowLeft}{/Shift}');
    expect(layout.split).toBe(42);
    await user.keyboard('{End}');
    expect(layout.split).toBe(80);
  });

  it('stacks the cards from the layout menu, and remembers it', async () => {
    const { user, layout } = await rendered();
    await user.click(screen.getByRole('treeitem', { name: 'Get a user' }));
    await user.click(screen.getByRole('button', { name: 'Layout' }));
    expect(screen.getByRole('menuitem', { name: /Follow the window/ }).textContent).toContain(
      'current',
    );
    await user.click(screen.getByRole('menuitem', { name: 'Stacked' }));
    expect(layout.orient).toBe('stack');
    const divider = screen.getByRole('separator', { name: 'Resize request and response' });
    expect(divider.getAttribute('aria-orientation')).toBe('horizontal');
  });

  it('opens the send menu from the keyboard and hands focus back on Escape', async () => {
    const { user } = await rendered();
    await user.click(screen.getByRole('treeitem', { name: 'Get a user' }));
    const more = screen.getByRole('button', { name: 'More send options' });
    await user.click(more);
    const items = within(screen.getByRole('menu')).getAllByRole('menuitem');
    expect(items.map((i) => i.textContent?.trim().split(/\s+/)[0])).toEqual([
      'Send',
      'Send',
      'Copy',
    ]);
    expect(document.activeElement).toBe(items[0]);
    await user.keyboard('{ArrowDown}');
    expect(document.activeElement).toBe(items[1]);
    await user.keyboard('{Escape}');
    expect(screen.queryByRole('menu')).toBeNull();
    expect(document.activeElement).toBe(more);
  });
});
