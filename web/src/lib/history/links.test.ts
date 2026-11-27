import { describe, expect, it } from 'vitest';
import { ago, compareHref, exports, lineCommand, runHref, span } from './links.ts';

const NOW = 1_790_000_000_000;
const MINUTE = 60_000;

describe('ago', () => {
  it('says when a run started, in the largest unit that fits', () => {
    expect(ago(NOW - 10_000, NOW)).toBe('just now');
    expect(ago(NOW - 5 * MINUTE, NOW)).toBe('5 min ago');
    expect(ago(NOW - 3 * 60 * MINUTE, NOW)).toBe('3 h ago');
    expect(ago(NOW - 2 * 24 * 60 * MINUTE, NOW)).toBe('2 d ago');
  });

  it('gives the date of a run older than a week', () => {
    const started = NOW - 10 * 24 * 60 * MINUTE;
    expect(ago(started, NOW)).toBe(new Date(started).toLocaleDateString());
  });
});

describe('span', () => {
  it('gives a run length in ms, seconds, or minutes and seconds', () => {
    expect(span(undefined)).toBe('—');
    expect(span(420)).toBe('420 ms');
    expect(span(1234)).toBe('1.2 s');
    expect(span(184_000)).toBe('3 min 4 s');
  });
});

describe('commands', () => {
  it('runs a stored selection again under its profiles', () => {
    expect(lineCommand({ selector: 'users.crud#a tag:smoke', profiles: ['staging', 'eu'] })).toBe(
      'vc run users.crud#a -t smoke --profile staging,eu',
    );
    expect(lineCommand({ selector: '', profiles: [] })).toBe('vc run');
  });

  it('copies a command or path for each export of a run', () => {
    expect(exports.map((e) => [e.id, e.command(7)])).toEqual([
      ['html', 'vc report 7 --format html'],
      ['ctrf', 'vc report 7 --format ctrf'],
      ['junit', 'vc report 7 --format junit'],
      ['json', 'vc history show 7 --json'],
      ['path', '.vibranium/reports/7/index.html'],
    ]);
  });
});

describe('hrefs', () => {
  it('opens a load run on the load screen and any other run on its own page', () => {
    expect(runHref({ id: 12, kind: 'load' })).toBe('/runs/load/12');
    expect(runHref({ id: 7, kind: 'run' })).toBe('/runs/7');
    expect(runHref({ id: 8, kind: 'adhoc' })).toBe('/runs/8');
  });

  it('compares the base run with the head run', () => {
    expect(compareHref(6, 7)).toBe('/runs/compare/6/7');
  });
});
