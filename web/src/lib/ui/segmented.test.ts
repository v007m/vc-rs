import { expect, it } from 'vitest';
import { lens } from './segmented.ts';

it('follows the pressed option, and shows no lens when none is pressed', async () => {
  const track = document.createElement('div');
  track.innerHTML =
    '<button aria-pressed="true">Pretty</button><button aria-pressed="false">Raw</button>';
  document.body.append(track);
  const buttons = track.querySelectorAll('button');
  const [first, second] = [buttons[0], buttons[1]];
  if (!first || !second) throw new Error('two buttons');
  Object.defineProperties(second, {
    offsetLeft: { value: 60 },
    offsetWidth: { value: 48 },
  });
  const stop = lens(track);
  expect(track.dataset['lens']).toBe('on');
  expect(track.style.getPropertyValue('--lens-x')).toBe('0px');

  first.setAttribute('aria-pressed', 'false');
  second.setAttribute('aria-pressed', 'true');
  await Promise.resolve();
  expect(track.style.getPropertyValue('--lens-x')).toBe('60px');
  expect(track.style.getPropertyValue('--lens-w')).toBe('48px');

  second.setAttribute('aria-pressed', 'false');
  await Promise.resolve();
  expect(track.dataset['lens']).toBe('none');

  stop();
  expect(track.dataset['lens']).toBeUndefined();
});
