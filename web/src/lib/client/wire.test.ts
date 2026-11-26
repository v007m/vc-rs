import { describe, expect, it } from 'vitest';
import { cookies, size, statusClass, urlParts } from './wire.ts';

describe('wire helpers', () => {
  it('splits a URL into text and the vars the engine fills in', () => {
    expect(urlParts('${baseUrl}/users/${ userId }?x=1')).toEqual([
      { text: '${baseUrl}', name: 'baseUrl' },
      { text: '/users/' },
      { text: '${ userId }', name: 'userId' },
      { text: '?x=1' },
    ]);
    expect(urlParts('')).toEqual([]);
  });

  it('reads the cookies a response set, with their scope, expiry and flags', () => {
    const headers: [string, string][] = [
      ['content-type', 'application/json'],
      ['Set-Cookie', 'sid=abc; Path=/; Domain=api.example.com; HttpOnly; Secure; SameSite=Lax'],
      ['set-cookie', 'theme=dark; Max-Age=60\nlang=en; Expires=Wed, 01 Jan 2031 00:00:00 GMT'],
      ['set-cookie', '; Path=/'],
    ];
    expect(cookies(headers)).toEqual([
      {
        name: 'sid',
        value: 'abc',
        domain: 'api.example.com',
        path: '/',
        expires: 'Session',
        flags: 'HttpOnly · Secure · SameSite=Lax',
      },
      { name: 'theme', value: 'dark', domain: '', path: '', expires: 'in 60 s', flags: '' },
      {
        name: 'lang',
        value: 'en',
        domain: '',
        path: '',
        expires: 'Wed, 01 Jan 2031 00:00:00 GMT',
        flags: '',
      },
    ]);
  });

  it('writes sizes in B, KB and MB, and a status as its class', () => {
    expect([size(512), size(1843), size(2.4 * 1024 * 1024)]).toEqual(['512 B', '1.8 KB', '2.4 MB']);
    expect([statusClass(201), statusClass(404), statusClass(503)]).toEqual(['2', '4', '5']);
  });
});
