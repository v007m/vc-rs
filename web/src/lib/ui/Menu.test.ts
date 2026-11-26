import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { createRawSnippet } from 'svelte';
import { afterEach, expect, it, vi } from 'vitest';
import Menu from './Menu.svelte';

const props = {
  label: 'More',
  items: [{ id: 'push', label: 'Push' }],
  onpick: () => undefined,
  children: createRawSnippet(() => ({ render: () => '<span>More</span>' })),
};

afterEach(() => vi.restoreAllMocks());

it('opens below its button when there is room', async () => {
  render(Menu, props);
  await userEvent.click(screen.getByRole('button', { name: 'More' }));
  expect(screen.getByRole('menu').classList.contains('up')).toBe(false);
});

it('opens upward when it would run off the bottom of the window', async () => {
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (
    this: HTMLElement,
  ) {
    const bottom = this.getAttribute('role') === 'menu' ? window.innerHeight + 40 : 0;
    return DOMRect.fromRect({ x: 0, y: bottom - 100, width: 100, height: 100 });
  });
  render(Menu, props);
  await userEvent.click(screen.getByRole('button', { name: 'More' }));
  expect(screen.getByRole('menu').classList.contains('up')).toBe(true);
});
