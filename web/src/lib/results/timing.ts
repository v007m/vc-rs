/** A call's timing in ms, as the engine measured it; a phase it did not measure is absent. */
export type Timing = Readonly<{
  dnsMs?: number;
  connectMs?: number;
  tlsMs?: number;
  firstByteMs: number;
  downloadMs: number;
  totalMs: number;
}>;

/** The phases of a call, in the order they happen. */
export const phaseNames = ['dns', 'connect', 'tls', 'firstByte', 'download'] as const;

export type PhaseName = (typeof phaseNames)[number];

/** One bar of the waterfall: where it starts and how wide it is, in % of the whole call. */
export type PhaseBar = Readonly<{ name: PhaseName; ms: number; start: number; width: number }>;

const field: Readonly<Record<PhaseName, keyof Timing>> = {
  dns: 'dnsMs',
  connect: 'connectMs',
  tls: 'tlsMs',
  firstByte: 'firstByteMs',
  download: 'downloadMs',
};

/** The measured phases, each starting where the one before it ended. */
export function waterfall(timing: Timing): PhaseBar[] {
  const measured = phaseNames.flatMap((name) => {
    const ms = timing[field[name]];
    return ms === undefined || ms < 0 ? [] : [{ name, ms }];
  });
  const sum = measured.reduce((total, phase) => total + phase.ms, 0);
  const whole = Math.max(timing.totalMs, sum);
  let at = 0;
  return measured.map(({ name, ms }) => {
    const bar = { name, ms, start: percent(at, whole), width: percent(ms, whole) };
    at += ms;
    return bar;
  });
}

/** Whether the call reused an open connection: no time went to dns, connect or tls. */
export function reused(timing: Timing): boolean {
  return (timing.dnsMs ?? 0) + (timing.connectMs ?? 0) + (timing.tlsMs ?? 0) === 0;
}

function percent(part: number, whole: number): number {
  return whole > 0 ? Math.min(100, (part / whole) * 100) : 0;
}
