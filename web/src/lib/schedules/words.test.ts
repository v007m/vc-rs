import { describe, expect, it } from 'vitest';
import { cronWords, untilWords } from './words.ts';

describe('cronWords', () => {
  it.each([
    ['* * * * *', 'Every minute'],
    ['*/15 * * * *', 'Every 15 minutes'],
    ['5 * * * *', 'Every hour at :05'],
    ['0 */6 * * *', 'Every 6 hours at :00'],
    ['0 2 * * *', 'Every day at 02:00 UTC'],
    ['30 6 * * 1-5', 'Weekdays at 06:30 UTC'],
    ['0 9 * * 0,6', 'Weekends at 09:00 UTC'],
    ['0 9 * * 1-3', 'Monday to Wednesday at 09:00 UTC'],
    ['30 6 * * 1', 'Every Monday at 06:30 UTC'],
    ['0 8 * * 1,4', 'Every Monday and Thursday at 08:00 UTC'],
    ['0 0 * * 7', 'Every Sunday at 00:00 UTC'],
    ['15 4 1 * *', 'Day 1 of each month at 04:15 UTC'],
    ['  0   2 * * *  ', 'Every day at 02:00 UTC'],
  ])('says %s as "%s"', (cron, words) => {
    expect(cronWords(cron)).toBe(words);
  });

  it.each([
    ['0 2 * 1 *', 'a month'],
    ['0 2 1 * 1', 'a day of the month and of the week'],
    ['0 25 * * *', 'an hour past 23'],
    ['61 * * * *', 'a minute past 59'],
    ['0 2 * * 8', 'a weekday past 7'],
    ['*/1 * * * *', 'a step of 1'],
    ['0 2 * *', 'four fields'],
    ['@daily', 'a macro'],
    ['', 'nothing'],
  ])('leaves %s as written (%s)', (cron) => {
    expect(cronWords(cron)).toBeUndefined();
  });
});

describe('untilWords', () => {
  const now = 1_795_700_000_000;

  it('says how soon, in the largest unit under a week', () => {
    expect(untilWords(now + 20_000, now)).toBe('now');
    expect(untilWords(now + 5 * 60_000, now)).toBe('in 5 min');
    expect(untilWords(now + 3 * 3_600_000, now)).toBe('in 3 h');
    expect(untilWords(now + 2 * 86_400_000, now)).toBe('in 2 d');
  });

  it('gives the date a week or more away', () => {
    const at = now + 9 * 86_400_000;
    expect(untilWords(at, now)).toBe(new Date(at).toLocaleDateString());
  });
});
