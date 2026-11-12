import {
  CheckKind,
  type Check,
  type Exchange,
  type Header,
} from '#lib/gen/vibranium/results/v1/results_pb.ts';
import type { CheckLine, ExchangeLine, LazyBody } from '#lib/results/types.ts';

/** What each kind of check is called when it names no header or body path. */
const kindLabel: Readonly<Record<CheckKind, string>> = {
  [CheckKind.UNSPECIFIED]: 'check',
  [CheckKind.STATUS]: 'status',
  [CheckKind.HEADER]: 'header',
  [CheckKind.SCHEMA]: 'schema',
  [CheckKind.MATCH]: 'match',
  [CheckKind.EQUALS]: 'equals',
  [CheckKind.SIZE]: 'size',
  [CheckKind.TIMING]: 'timing',
  [CheckKind.CEL]: 'expression',
  [CheckKind.CUSTOM]: 'custom',
};

const decoder = new TextDecoder();

function pairs(headers: readonly Header[]): [string, string][] {
  return headers.map(({ name, value }) => [name, value]);
}

function body(bytes: Uint8Array): LazyBody | undefined {
  if (bytes.length === 0) return undefined;
  const text = decoder.decode(bytes);
  return () => Promise.resolve(text);
}

/** The engine's exchange as a view shows it; its values are already masked by the engine. */
export function exchangeLine(exchange: Exchange): ExchangeLine {
  const { timing } = exchange;
  return {
    method: exchange.method,
    url: exchange.url,
    status: exchange.status,
    ms: timing?.totalMs,
    timing: timing && {
      dnsMs: timing.dnsMs,
      connectMs: timing.connectMs,
      tlsMs: timing.tlsMs,
      firstByteMs: timing.firstByteMs,
      downloadMs: timing.downloadMs,
      totalMs: timing.totalMs,
    },
    requestHeaders: pairs(exchange.requestHeaders),
    responseHeaders: pairs(exchange.responseHeaders),
    requestBody: body(exchange.requestBody),
    responseBody: body(exchange.responseBody),
    truncated: exchange.bodyTruncated,
  };
}

/** The engine's check as a view shows it: what it checked, and why it could not be evaluated. */
export function checkLine(check: Check): CheckLine {
  const { span } = check;
  return {
    label: check.target ?? kindLabel[check.kind] ?? 'check',
    passed: check.passed,
    expected: check.expected,
    actual: check.actual ?? check.error,
    location: span?.file ? `${span.file}:${span.startLine}` : undefined,
  };
}
