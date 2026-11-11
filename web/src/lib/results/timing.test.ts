import { render, screen, within } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import { reused, waterfall } from './timing.ts';
import Waterfall from './Waterfall.svelte';

// A first call to the seed project's users api over TLS, as the engine measured it.
const fresh = {
  dnsMs: 2,
  connectMs: 8,
  tlsMs: 20,
  firstByteMs: 60,
  downloadMs: 10,
  totalMs: 100,
};

// The next call on the same connection: no dns, connect or tls.
const kept = { firstByteMs: 30, downloadMs: 10, totalMs: 40 };

describe('waterfall', () => {
  it('lays each phase after the one before it, in % of the whole call', () => {
    expect(waterfall(fresh)).toEqual([
      { name: 'dns', ms: 2, start: 0, width: 2 },
      { name: 'connect', ms: 8, start: 2, width: 8 },
      { name: 'tls', ms: 20, start: 10, width: 20 },
      { name: 'firstByte', ms: 60, start: 30, width: 60 },
      { name: 'download', ms: 10, start: 90, width: 10 },
    ]);
  });

  it('leaves out the phases a reused connection never went through', () => {
    expect(waterfall(kept).map((bar) => bar.name)).toEqual(['firstByte', 'download']);
    expect(reused(kept)).toBe(true);
    expect(reused(fresh)).toBe(false);
  });

  it('scales by the phases when they add up to more than the total, and never past 100%', () => {
    const bars = waterfall({ firstByteMs: 30, downloadMs: 30, totalMs: 0 });
    expect(bars.map(({ start, width }) => [start, width])).toEqual([
      [0, 50],
      [50, 50],
    ]);
    expect(waterfall({ firstByteMs: 0, downloadMs: 0, totalMs: 0 })[0]?.width).toBe(0);
  });
});

describe('Waterfall', () => {
  it('names each phase with its time, then the total', () => {
    render(Waterfall, { timing: fresh });
    const rows = within(screen.getByRole('table', { name: 'Timing waterfall' })).getAllByRole(
      'row',
    );
    const cells = (row: HTMLElement): string =>
      Array.from(row.querySelectorAll('th, td'), (cell) => cell.textContent?.trim())
        .filter(Boolean)
        .join(' ');
    expect(rows.map(cells)).toEqual([
      'dns 2.0 ms',
      'connect 8.0 ms',
      'tls 20 ms',
      'firstByte 60 ms',
      'download 10 ms',
      'total 100 ms',
    ]);
    expect(screen.queryByText(/Connection reused/)).toBeNull();
  });

  it('says when the connection was reused', () => {
    render(Waterfall, { timing: kept });
    expect(screen.getByText(/Connection reused from an earlier request/)).toBeTruthy();
  });
});
