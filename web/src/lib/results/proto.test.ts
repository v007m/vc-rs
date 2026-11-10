import { create } from '@bufbuild/protobuf';
import { describe, expect, it } from 'vitest';
import {
  CheckKind,
  CheckSchema,
  ExchangeSchema,
} from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { checkLine, exchangeLine, timingMs } from './proto.ts';

describe('results from the engine', () => {
  it('labels a check by its kind and target', () => {
    const header = create(CheckSchema, {
      kind: CheckKind.HEADER,
      target: 'content-type',
      origin: 'users#getUser',
      passed: false,
      expected: '"application/json"',
      actual: '"text/html"',
    });
    expect(checkLine(header)).toEqual({
      label: 'header content-type',
      passed: false,
      expected: '"application/json"',
      actual: '"text/html"',
      location: 'users#getUser',
    });
    const status = checkLine(create(CheckSchema, { kind: CheckKind.STATUS, passed: true }));
    expect([status.label, status.location]).toEqual(['status', undefined]);
  });

  it('reads an exchange with its bodies in hand and its timing', async () => {
    const exchange = create(ExchangeSchema, {
      method: 'POST',
      url: 'http://127.0.0.1:4010/users',
      status: 201,
      timing: { connectMs: 2, firstByteMs: 9, downloadMs: 1, totalMs: 12 },
      requestHeaders: [{ name: 'authorization', value: '••••' }],
      requestBody: new TextEncoder().encode('{"email":"a@b.test"}'),
    });
    const line = exchangeLine(exchange);
    expect(line).toMatchObject({ status: 201, ms: 12, truncated: false, responseBody: undefined });
    expect(line.requestHeaders).toEqual([['authorization', '••••']]);
    expect(await line.requestBody?.()).toBe('{"email":"a@b.test"}');
    expect(timingMs(exchange.timing)).toEqual({
      dnsMs: undefined,
      connectMs: 2,
      tlsMs: undefined,
      firstByteMs: 9,
      downloadMs: 1,
      totalMs: 12,
    });
    expect(timingMs(undefined).totalMs).toBe(0);
  });
});
