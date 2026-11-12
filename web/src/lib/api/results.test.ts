import { create } from '@bufbuild/protobuf';
import { describe, expect, it } from 'vitest';
import {
  CheckKind,
  CheckSchema,
  ExchangeSchema,
} from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { checkLine, exchangeLine } from './results.ts';

const encoder = new TextEncoder();

describe('exchangeLine', () => {
  it('keeps the timing, the headers and the bodies, loaded when asked for', async () => {
    const line = exchangeLine(
      create(ExchangeSchema, {
        action: 'call users#createUser',
        method: 'POST',
        url: 'http://localhost:7421/users',
        status: 201,
        timing: { connectMs: 1.5, firstByteMs: 12, downloadMs: 0.5, totalMs: 14 },
        requestHeaders: [{ name: 'authorization', value: 'Bearer ••••' }],
        requestBody: encoder.encode('{"email":"grace@example.com"}'),
        bodyTruncated: true,
      }),
    );
    expect(line).toMatchObject({
      method: 'POST',
      status: 201,
      ms: 14,
      timing: { connectMs: 1.5, firstByteMs: 12, downloadMs: 0.5, totalMs: 14 },
      requestHeaders: [['authorization', 'Bearer ••••']],
      responseHeaders: [],
      responseBody: undefined,
      truncated: true,
    });
    expect(line.timing?.dnsMs).toBeUndefined();
    expect(await line.requestBody?.()).toBe('{"email":"grace@example.com"}');
  });

  it('has no status or timing when no response came back', () => {
    const line = exchangeLine(create(ExchangeSchema, { method: 'GET', url: 'http://x/' }));
    expect([line.status, line.ms, line.timing]).toEqual([undefined, undefined, undefined]);
  });
});

describe('checkLine', () => {
  it('names a check by its target, or by its kind when it has none', () => {
    const status = checkLine(
      create(CheckSchema, {
        kind: CheckKind.STATUS,
        passed: false,
        expected: '201',
        actual: '500',
        span: { file: 'services/users/users.api.yaml', startLine: 14 },
      }),
    );
    expect(status).toEqual({
      label: 'status',
      passed: false,
      expected: '201',
      actual: '500',
      location: 'services/users/users.api.yaml:14',
    });
    const field = checkLine(
      create(CheckSchema, { kind: CheckKind.MATCH, target: 'body.email', passed: true }),
    );
    expect([field.label, field.location]).toEqual(['body.email', undefined]);
  });

  it('shows why a check could not be evaluated as what came back', () => {
    const line = checkLine(
      create(CheckSchema, { kind: CheckKind.CEL, expected: 'true', error: 'no field `id`' }),
    );
    expect([line.label, line.actual]).toEqual(['expression', 'no field `id`']);
  });
});
