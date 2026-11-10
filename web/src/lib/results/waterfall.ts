/** One exchange's timing in ms, as `results.v1.Timing` carries it. */
export type TimingMs = Readonly<{
  dnsMs?: number;
  connectMs?: number;
  tlsMs?: number;
  /** From the start of the send to the response head. */
  firstByteMs: number;
  /** Reading the body after the head. */
  downloadMs: number;
  totalMs: number;
}>;

/** One bar of the waterfall; `start` and `width` are percentages of the whole. */
export type Phase = Readonly<{ label: string; ms: number; start: number; width: number }>;

/**
 * The phases that took time, in the order they happen, each starting where the one before
 * ended. A phase that did not happen (a reused connection has no DNS, connect or TLS) or took
 * no time is left out. The scale is the total, or the phases' sum when that is longer.
 */
export function phases(timing: TimingMs): Phase[] {
  const spans: [string, number | undefined][] = [
    ['dns', timing.dnsMs],
    ['connect', timing.connectMs],
    ['tls', timing.tlsMs],
    ['firstByte', timing.firstByteMs],
    ['download', timing.downloadMs],
  ];
  const taken = spans.filter((span): span is [string, number] => (span[1] ?? 0) > 0);
  const sum = taken.reduce((total, [, ms]) => total + ms, 0);
  const scale = Math.max(timing.totalMs, sum);
  let at = 0;
  return taken.map(([label, ms]) => {
    const phase = { label, ms, start: (at * 100) / scale, width: (ms * 100) / scale };
    at += ms;
    return phase;
  });
}

/** Whether the exchange reused an open connection: no DNS, connect or TLS took time. */
export function reused(timing: TimingMs): boolean {
  return !timing.dnsMs && !timing.connectMs && !timing.tlsMs;
}
