import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import CheckList from './CheckList.svelte';
import ExchangeView from './ExchangeView.svelte';
import TestList from './TestList.svelte';
import type { ExchangeLine, TestRow } from './types.ts';

/** The element's text with runs of whitespace collapsed, as a reader hears it. */
const text = (element: Element | undefined): string =>
  (element?.textContent ?? '').replace(/\s+/g, ' ').trim();

describe('CheckList', () => {
  it('names each check and shows expected and actual for a failure', () => {
    render(CheckList, {
      checks: [
        {
          label: 'status',
          passed: false,
          expected: '200',
          actual: '500',
          location: 'a.test.yaml:4',
        },
        { label: 'body.id', passed: true, expected: 'present' },
      ],
    });
    const [failed, held] = within(screen.getByRole('list', { name: 'Checks' })).getAllByRole(
      'listitem',
    );
    expect(text(failed)).toContain('status failed');
    expect(text(failed)).toContain('expected 200');
    expect(text(failed)).toContain('actual 500');
    expect(text(failed)).toContain('a.test.yaml:4');
    expect(text(held)).toContain('body.id held');
    expect(text(held)).not.toContain('expected');
  });
});

describe('ExchangeView', () => {
  const exchange = (body: string, loads: string[]): ExchangeLine => ({
    method: 'GET',
    url: 'http://localhost/users/9',
    status: 500,
    ms: 12,
    requestHeaders: [['accept', 'application/json']],
    responseHeaders: [],
    responseBody: () => {
      loads.push(body);
      return Promise.resolve(body);
    },
    truncated: false,
  });

  it('loads the body only when opened and shows it as text', async () => {
    const loads: string[] = [];
    render(ExchangeView, { exchange: exchange('<script>alert(1)</script>', loads) });
    expect(screen.getByText('→ 500 · 12 ms')).toBeTruthy();
    expect(loads).toEqual([]);
    await userEvent.setup().click(screen.getByText('GET http://localhost/users/9'));
    expect(await screen.findByText('<script>alert(1)</script>')).toBeTruthy();
    expect(document.querySelector('pre script')).toBeNull();
    expect(text(screen.getByRole('table', { name: 'Request headers' }))).toContain('accept');
    expect(loads).toHaveLength(1);
  });

  it('pretty-prints a JSON body', async () => {
    render(ExchangeView, { exchange: exchange('{"error":"boom"}', []) });
    await userEvent.setup().click(screen.getByText('GET http://localhost/users/9'));
    expect(await screen.findByText(/"error": "boom"/)).toBeTruthy();
  });
});

describe('TestList', () => {
  // jsdom has no layout, so `bind:clientHeight` needs a ResizeObserver that never fires.
  beforeAll(() => {
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe(): void {}
        unobserve(): void {}
        disconnect(): void {}
      },
    );
  });

  const rows: TestRow[] = Array.from({ length: 10_000 }, (_, i) => ({
    key: String(i),
    label: `Checks thing ${i}`,
    file: 'users.crud',
    status: i % 10 === 9 ? 'failed' : 'passed',
    ms: 4,
    tags: [],
  }));

  function rendered() {
    const opened: string[] = [];
    render(TestList, {
      label: 'Tests',
      rows,
      current: undefined,
      onopen: (key) => opened.push(key),
    });
    return {
      opened,
      user: userEvent.setup(),
      list: screen.getByRole('listbox', { name: 'Tests' }),
    };
  }

  it('renders only a window of a long list, with its position in the set', () => {
    const { list } = rendered();
    const options = within(list).getAllByRole('option');
    expect(options.length).toBeLessThan(60);
    expect(text(options[9])).toBe('Checks thing 9 , failed users.crud 4.0');
    expect(options[0]?.getAttribute('aria-setsize')).toBe('10000');
  });

  it('moves the active row with the keyboard and opens it with Enter', async () => {
    const { list, opened, user } = rendered();
    list.focus();
    await user.keyboard('{ArrowDown}{ArrowDown}');
    expect(list.getAttribute('aria-activedescendant')).toMatch(/-2$/);
    await user.keyboard('{Enter}');
    expect(opened).toEqual(['2']);
  });

  it('opens a clicked row', async () => {
    const { opened, user } = rendered();
    await user.click(screen.getByText('Checks thing 7'));
    expect(opened).toEqual(['7']);
  });
});
