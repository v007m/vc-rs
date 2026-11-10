import {
  CheckKind,
  type Check,
  type Exchange,
  type Header,
  type Timing,
} from '#lib/gen/vibranium/results/v1/results_pb.ts';
import type { CheckLine, ExchangeLine, LazyBody } from './types.ts';
import type { TimingMs } from './waterfall.ts';

const kindNames: Readonly<Record<CheckKind, string>> = {
  [CheckKind.UNSPECIFIED]: 'check',
  [CheckKind.STATUS]: 'status',
  [CheckKind.HEADER]: 'header',
  [CheckKind.SCHEMA]: 'schema',
  [CheckKind.MATCH]: 'match',
  [CheckKind.EQUALS]: 'equals',
  [CheckKind.SIZE]: 'size',
  [CheckKind.TIMING]: 'timing',
  [CheckKind.CEL]: 'cel',
  [CheckKind.CUSTOM]: 'custom',
};

/** A check as the check list shows it: `header content-type`, `match body.id`, `status`. */
export function checkLine(check: Check): CheckLine {
  const kind = kindNames[check.kind] ?? 'check';
  return {
    label: check.target === undefined ? kind : `${kind} ${check.target}`,
    passed: check.passed,
    expected: check.expected,
    actual: check.actual,
    location: check.origin || undefined,
  };
}

/** An exchange's timing in ms; a missing one is all zero. */
export function timingMs(timing: Timing | undefined): TimingMs {
  return {
    dnsMs: timing?.dnsMs,
    connectMs: timing?.connectMs,
    tlsMs: timing?.tlsMs,
    firstByteMs: timing?.firstByteMs ?? 0,
    downloadMs: timing?.downloadMs ?? 0,
    totalMs: timing?.totalMs ?? 0,
  };
}

const pairs = (headers: readonly Header[]): [string, string][] =>
  headers.map(({ name, value }) => [name, value]);

/** A body already in hand, as text; none when it is empty. */
function inHand(bytes: Uint8Array): LazyBody | undefined {
  if (bytes.length === 0) return undefined;
  const text = new TextDecoder().decode(bytes);
  return () => Promise.resolve(text);
}

/** An exchange whose bodies came with it, as the exchange view shows it. */
export function exchangeLine(exchange: Exchange): ExchangeLine {
  return {
    method: exchange.method,
    url: exchange.url,
    status: exchange.status,
    ms: exchange.timing?.totalMs,
    requestHeaders: pairs(exchange.requestHeaders),
    responseHeaders: pairs(exchange.responseHeaders),
    requestBody: inHand(exchange.requestBody),
    responseBody: inHand(exchange.responseBody),
    truncated: exchange.bodyTruncated,
  };
}
