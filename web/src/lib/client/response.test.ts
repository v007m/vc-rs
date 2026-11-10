import { describe, expect, it } from 'vitest';
import { headline, parseResponse, scalarFields, varArg } from './response.ts';

describe('parseResponse', () => {
  it('reads the status, headers and body', () => {
    const response = parseResponse(
      JSON.stringify({
        status: 201,
        headers: { 'content-type': 'application/json', vary: ['accept', 'origin'] },
        body: { id: 'u_1' },
      }),
    );
    expect(response).toEqual({
      status: 201,
      headers: [
        ['content-type', 'application/json'],
        ['vary', 'accept, origin'],
      ],
      body: { id: 'u_1' },
    });
  });

  it('gives undefined for nothing, bad JSON or no status', () => {
    expect(parseResponse(undefined)).toBeUndefined();
    expect(parseResponse('{')).toBeUndefined();
    expect(parseResponse('{"body":1}')).toBeUndefined();
  });
});

describe('scalarFields', () => {
  it('lists the paths of scalars only, skipping keys a dotted path cannot name', () => {
    const body = { id: 'u_1', tags: ['a'], profile: { email: 'e', 'a.b': 1 }, items: [{ n: 1 }] };
    expect(scalarFields(body)).toEqual([
      'body.id',
      'body.tags[0]',
      'body.profile.email',
      'body.items[0].n',
    ]);
  });

  it('offers nothing for a scalar body', () => {
    expect(scalarFields('ok')).toEqual([]);
  });
});

it('takes the first line as the headline and writes vars as name=value', () => {
  expect(headline('200 OK · 148 ms · api expect passed\n{}')).toBe(
    '200 OK · 148 ms · api expect passed',
  );
  expect(varArg('email', 'a=b@x')).toBe('email=a=b@x');
});
