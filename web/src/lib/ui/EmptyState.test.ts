import { render, screen } from '@testing-library/svelte';
import { expect, it } from 'vitest';
import EmptyState from './EmptyState.svelte';

it('shows a title and caption in a wash, with its icon hidden from assistive tech', () => {
  const { container } = render(EmptyState, {
    icon: 'send',
    title: 'No response yet',
    caption: 'Send the request.',
  });
  expect(screen.getByText('No response yet').classList.contains('title')).toBe(true);
  expect(screen.getByText('Send the request.')).toBeTruthy();
  expect(container.querySelector('.empty.vc-wash')).toBeTruthy();
});
