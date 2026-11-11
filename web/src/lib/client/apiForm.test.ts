import { describe, expect, it } from 'vitest';
import { MASKED } from '#lib/settings/port.ts';
import {
  bodyProblem,
  checkEdits,
  checkProblem,
  checkText,
  editsBetween,
  expectLines,
  formOf,
  type ApiForm,
} from './apiForm.ts';

const AT = '/apis/1';

const API = {
  id: 'get',
  path: '/users/${var.userId}',
  query: { limit: 10, q: '${var.q}' },
  headers: { 'X-Key': MASKED, Accept: 'application/json' },
  body: { name: '${var.name}' },
  expect: { status: 200, body: { match: { '$.id': { exists: true } } } },
};

const base = (): ApiForm => formOf(API);

describe('formOf', () => {
  it('reads the api as written, GET when it names no method', () => {
    expect(base()).toEqual({
      method: 'GET',
      path: '/users/${var.userId}',
      query: [
        { name: 'limit', value: '10', on: true },
        { name: 'q', value: '${var.q}', on: true },
      ],
      headers: [
        { name: 'X-Key', value: MASKED, on: true },
        { name: 'Accept', value: 'application/json', on: true },
      ],
      bodyMode: 'json',
      body: '{\n  "name": "${var.name}"\n}',
    });
    expect(formOf({ path: '/x', body: 'hi' })).toMatchObject({ bodyMode: 'text', body: 'hi' });
    expect(formOf({ path: '/x' })).toMatchObject({ query: [], bodyMode: 'none', body: '' });
  });
});

describe('editsBetween', () => {
  it('gives nothing for an unchanged form', () => {
    expect(editsBetween(AT, base(), base())).toEqual([]);
  });

  it('sets the method and path, and each query and header that changed', () => {
    const form = base();
    form.method = 'POST';
    form.path = '/users';
    form.query[0] = { name: 'limit', value: '20', on: true };
    form.query[1] = { name: 'q', value: '${var.q}', on: false };
    form.headers.push(
      { name: 'X-Trace/Id', value: 't1', on: true },
      { name: ' ', value: 'x', on: true },
    );
    expect(editsBetween(AT, base(), form)).toEqual([
      { pointer: '/apis/1/method', op: 'set', value: 'POST' },
      { pointer: '/apis/1/path', op: 'set', value: '/users' },
      { pointer: '/apis/1/query/q', op: 'remove' },
      { pointer: '/apis/1/query/limit', op: 'set', value: '20' },
      { pointer: '/apis/1/headers/X-Trace~1Id', op: 'set', value: 't1' },
    ]);
  });

  it('never writes the mask back, and removes a map left empty whole', () => {
    const form = base();
    form.headers[0] = { name: 'X-Key', value: `${MASKED}x`, on: true };
    form.query = [];
    expect(editsBetween(AT, base(), form)).toEqual([{ pointer: '/apis/1/query', op: 'remove' }]);
    form.headers[0] = { name: 'X-Key', value: MASKED, on: false };
    expect(editsBetween(AT, base(), form)).toContainEqual({
      pointer: '/apis/1/headers/X-Key',
      op: 'remove',
    });
  });

  it('replaces the body as JSON or text, and removes it for none', () => {
    const form = base();
    form.body = '{"name": "Ada", "tags": []}';
    expect(editsBetween(AT, base(), form)).toEqual([
      { pointer: '/apis/1/body', op: 'remove' },
      { pointer: '/apis/1/body', op: 'set', value: { name: 'Ada', tags: [] } },
    ]);
    form.bodyMode = 'none';
    expect(editsBetween(AT, base(), form)).toEqual([{ pointer: '/apis/1/body', op: 'remove' }]);
    const text = formOf({ path: '/x', body: 'hi' });
    expect(editsBetween(AT, text, { ...text, body: 'bye' })).toEqual([
      { pointer: '/apis/1/body', op: 'set', value: 'bye' },
    ]);
    expect(editsBetween(AT, formOf({ path: '/x' }), { ...text, body: 'new' })).toEqual([
      { pointer: '/apis/1/body', op: 'set', value: 'new' },
    ]);
  });

  it('says why a JSON body will not do, and edits no body that holds the mask', () => {
    const form = base();
    form.body = '{"name": ';
    expect(bodyProblem(form)).toBe('The body is not JSON. Fix it, or send it as text.');
    expect(editsBetween(AT, base(), form)).toEqual([]);
    form.body = `{"key": "${MASKED}"}`;
    expect(bodyProblem(form)).toBeUndefined();
    expect(editsBetween(AT, base(), form)).toEqual([]);
  });
});

describe('added checks', () => {
  it('are sent under the api’s expect, replacing a check it has', () => {
    const checks = [
      { kind: 'status', path: '', op: 'equals', value: '201' },
      { kind: 'body', path: '$.id', op: 'exists', value: 'false' },
      { kind: 'body', path: '$.name', op: 'contains', value: 'Ada' },
      { kind: 'timing', path: 'total', op: 'lt', value: '500ms' },
    ] as const;
    expect(checkEdits(AT, API, checks)).toEqual([
      { pointer: '/apis/1/expect/status', op: 'remove' },
      { pointer: '/apis/1/expect/status', op: 'set', value: 201 },
      { pointer: '/apis/1/expect/body/match/$.id', op: 'remove' },
      { pointer: '/apis/1/expect/body/match/$.id', op: 'set', value: { exists: false } },
      { pointer: '/apis/1/expect/body/match/$.name', op: 'set', value: { contains: 'Ada' } },
      { pointer: '/apis/1/expect/timing/total', op: 'set', value: { lt: '500ms' } },
    ]);
    expect(checks.map(checkText)).toEqual([
      'status equals 201',
      'body $.id exists false',
      'body $.name contains Ada',
      'timing total lt 500ms',
    ]);
  });

  it('say why one will not do', () => {
    expect(checkProblem({ kind: 'status', path: '', op: 'equals', value: '2xx' })).toBeUndefined();
    expect(checkProblem({ kind: 'status', path: '', op: 'equals', value: 'ok' })).toBe(
      'A status is a code such as 201, or a class such as 2xx.',
    );
    expect(checkProblem({ kind: 'body', path: ' ', op: 'equals', value: '1' })).toBe(
      'Name the body field, such as $.id.',
    );
    expect(checkProblem({ kind: 'timing', path: 'ttfb', op: 'lt', value: '1s' })).toContain(
      'total',
    );
    expect(checkProblem({ kind: 'body', path: '$.n', op: 'gt', value: '' })).toBe(
      'Give the value to compare with.',
    );
  });

  it('shows the api’s own checks a line each', () => {
    expect(expectLines(API)).toEqual(['status: 200', 'body: {"match":{"$.id":{"exists":true}}}']);
    expect(expectLines({ path: '/x' })).toEqual([]);
  });
});
