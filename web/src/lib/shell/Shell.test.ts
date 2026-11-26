import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { createRawSnippet, tick } from 'svelte';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { RunSession } from '#lib/run/session.svelte.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import type { PrefStore } from './prefs.ts';
import { ProjectStatus } from './project.svelte.ts';
import Shell from './Shell.svelte';
import { ShellSlots } from './slots.svelte.ts';
import { ShellState } from './state.svelte.ts';

function memory(): PrefStore {
  const items = new Map<string, string>();
  return {
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
  };
}

// jsdom has no layout, so `bind:clientWidth` needs a ResizeObserver that never fires.
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

const body = createRawSnippet(() => ({ render: () => '<p>screen body</p>' }));

function setup(pathname: string, store: PrefStore = memory(), slots = new ShellSlots()) {
  const engine = fakeEngine({});
  const shell = new ShellState(store);
  const navigate = vi.fn<(href: string) => void>();
  const props = {
    shell,
    session: new RunSession(engine),
    project: new ProjectStatus(engine),
    pathname,
    navigate,
    slots,
    children: body,
  };
  const view = render(Shell, props);
  const rail = () => screen.getByRole('navigation', { name: 'Spaces' });
  return { ...view, shell, navigate, rail, user: userEvent.setup(), props, store };
}

describe('Shell', () => {
  it('shows the five spaces, marks the current one and titles the view', () => {
    const { rail } = setup('/runs/overview');
    const links = within(rail()).getAllByRole('link').slice(1);
    expect(links.map((a) => a.getAttribute('aria-label'))).toEqual([
      'Client',
      'Tests',
      'Runs',
      'Agents',
      'Settings',
    ]);
    expect(within(rail()).getByRole('link', { name: 'Runs' }).getAttribute('aria-current')).toBe(
      'page',
    );
    expect(within(rail()).getByRole('link', { name: 'Tests' }).hasAttribute('aria-current')).toBe(
      false,
    );
    expect(screen.getByRole('heading', { level: 1, name: 'Runs overview' })).toBeTruthy();
    expect(screen.getByRole('main').textContent).toContain('screen body');
  });

  it('links each space to its last screen, and the current one to its home', async () => {
    const { rail, rerender, props } = setup('/tests/editor');
    await rerender({ ...props, pathname: '/client' });
    expect(within(rail()).getByRole('link', { name: 'Tests' }).getAttribute('href')).toBe(
      '/tests/editor',
    );
    expect(within(rail()).getByRole('link', { name: 'Client' }).getAttribute('href')).toBe(
      '/client',
    );
  });

  it('switches spaces with ⌘1–⌘5 and ⌘,', async () => {
    const { navigate, user } = setup('/client');
    await user.keyboard('{Meta>}2{/Meta}');
    expect(navigate).toHaveBeenLastCalledWith('/tests');
    await user.keyboard('{Control>}4{/Control}');
    expect(navigate).toHaveBeenLastCalledWith('/agents');
    await user.keyboard('{Meta>},{/Meta}');
    expect(navigate).toHaveBeenLastCalledWith('/settings');
    await user.keyboard('3');
    expect(navigate).toHaveBeenCalledTimes(3);
  });

  it('paints each space in its Strata preset (D591)', async () => {
    const { container, rerender, props } = setup('/client');
    const app = () => container.querySelector('.app');
    expect(app()?.classList.contains('st-canvas')).toBe(true);
    const presets: string[] = [];
    for (const pathname of ['/client', '/tests', '/runs', '/agents', '/settings']) {
      await rerender({ ...props, pathname });
      presets.push(app()?.getAttribute('data-space') ?? '');
    }
    expect(presets).toEqual(['tide', 'lilac', 'ember', 'grove', 'graphite']);
  });

  it('shows the space shortcuts while the modifier is held, as plain text (D600)', async () => {
    vi.useFakeTimers();
    try {
      const { container, rail } = setup('/client');
      const app = container.querySelector('.app');
      const hints = [...rail().querySelectorAll('.sp-hint')].map((hint) => hint.textContent);
      expect(hints).toEqual(['Ctrl+1', 'Ctrl+2', 'Ctrl+3', 'Ctrl+4', 'Ctrl+,']);
      expect(rail().querySelector('.st-kbd')).toBeNull();
      await fireEvent.keyDown(window, { key: 'Control' });
      vi.advanceTimersByTime(450);
      await tick();
      expect(app?.classList.contains('is-hints')).toBe(true);
      await fireEvent.keyUp(window, { key: 'Control' });
      expect(app?.classList.contains('is-hints')).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('cycles the rail, sidebar, top row and screen with F6 and Shift+F6', async () => {
    const { rail, user } = setup('/tests');
    const sidebar = screen.getByRole('complementary', { name: 'Tests sidebar' });
    await user.keyboard('{F6}');
    expect(rail().contains(document.activeElement)).toBe(true);
    await user.keyboard('{F6}');
    expect(sidebar.contains(document.activeElement)).toBe(true);
    await user.keyboard('{Shift>}{F6}{/Shift}');
    expect(rail().contains(document.activeElement)).toBe(true);
    await user.keyboard('{Shift>}{F6}{/Shift}');
    expect(document.activeElement).toBe(screen.getByRole('main'));
  });

  it('closes a sidebar section and keeps it closed on the next visit', async () => {
    const { user, store, unmount } = setup('/tests');
    const head = screen.getByRole('button', { name: 'Tests' });
    expect(head.getAttribute('aria-expanded')).toBe('true');
    await user.click(head);
    expect(head.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByRole('link', { name: 'Editor' })).toBeNull();
    unmount();
    setup('/tests', store);
    expect(screen.getByRole('button', { name: 'Tests' }).getAttribute('aria-expanded')).toBe(
      'false',
    );
  });

  it('hides and shows the sidebar', async () => {
    const { user, shell } = setup('/agents');
    await user.click(screen.getByRole('button', { name: 'Hide sidebar' }));
    expect(shell.prefs.sbCollapsed).toBe(true);
    expect(screen.getByRole('button', { name: 'Show sidebar' }).getAttribute('aria-expanded')).toBe(
      'false',
    );
    await user.click(screen.getByRole('button', { name: 'Show sidebar' }));
    expect(shell.prefs.sbCollapsed).toBe(false);
  });

  it('moves the switcher to the sidebar foot when chosen', () => {
    const store = memory();
    new ShellState(store).setSwitcher('foot');
    setup('/client', store);
    expect(screen.getAllByRole('navigation', { name: 'Spaces' })).toHaveLength(2);
  });

  it("puts a screen's sidebar, top row and bare stage in place, and gives them back", async () => {
    const slots = new ShellSlots();
    const empty = slots.fill({
      sidebar: createRawSnippet(() => ({ render: () => '<p>own sidebar</p>' })),
      top: createRawSnippet(() => ({ render: () => '<nav aria-label="Open tabs"></nav>' })),
      bare: true,
    });
    const { container } = setup('/client', memory(), slots);
    expect(screen.getByText('own sidebar')).toBeTruthy();
    expect(screen.getByRole('navigation', { name: 'Open tabs' })).toBeTruthy();
    expect(screen.getByRole('heading', { level: 1 }).className).toBe('vc-sr');
    expect(container.querySelector('.stage > .card')).toBeNull();
    empty();
    await tick();
    expect(screen.queryByText('own sidebar')).toBeNull();
    expect(screen.getByRole('link', { name: 'Request' })).toBeTruthy();
    expect(container.querySelector('.stage > .card')).not.toBeNull();
  });

  it('keeps the screen mounted when it asks for a bare stage', async () => {
    let mounts = 0;
    const counted = createRawSnippet(() => ({
      render: () => '<p>counted screen</p>',
      setup: () => void (mounts += 1),
    }));
    const slots = new ShellSlots();
    const engine = fakeEngine({});
    render(Shell, {
      shell: new ShellState(memory()),
      session: new RunSession(engine),
      project: new ProjectStatus(engine),
      pathname: '/client',
      navigate: vi.fn<(href: string) => void>(),
      slots,
      children: counted,
    });
    const before = screen.getByText('counted screen');
    slots.fill({ bare: true });
    await tick();
    expect(mounts).toBe(1);
    expect(screen.getByText('counted screen')).toBe(before);
  });
});
