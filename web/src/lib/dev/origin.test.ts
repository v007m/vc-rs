import { describe, expect, it } from 'vitest';
import { fromDevPage } from './origin.ts';

const host = 'localhost:5173';

describe('fromDevPage', () => {
  it('lets the dev page call its own /api', () => {
    expect(
      fromDevPage({ host, origin: 'http://localhost:5173', 'sec-fetch-site': 'same-origin' }),
    ).toBe(true);
    expect(fromDevPage({ host, 'sec-fetch-site': 'same-origin' })).toBe(true);
    expect(fromDevPage({ host: '127.0.0.1:5173', origin: 'http://127.0.0.1:5173' })).toBe(true);
    expect(fromDevPage({ host: '[::1]:5173', origin: 'http://[::1]:5173' })).toBe(true);
  });

  it('refuses a page on another localhost port', () => {
    expect(
      fromDevPage({ host, origin: 'http://localhost:3000', 'sec-fetch-site': 'same-site' }),
    ).toBe(false);
    expect(fromDevPage({ host, 'sec-fetch-site': 'same-site' })).toBe(false);
    expect(fromDevPage({ host, origin: 'http://127.0.0.1:5173' })).toBe(false);
  });

  it('refuses a cross-site page and a request that is not from a page', () => {
    expect(fromDevPage({ host, origin: 'https://evil.test', 'sec-fetch-site': 'cross-site' })).toBe(
      false,
    );
    expect(fromDevPage({ host, origin: 'null' })).toBe(false);
    expect(fromDevPage({ host })).toBe(false);
    expect(fromDevPage({ host, origin: ['http://localhost:5173', 'http://localhost:5173'] })).toBe(
      false,
    );
  });

  it('refuses a LAN address and a DNS name rebound to loopback', () => {
    expect(fromDevPage({ host: '192.168.1.5:5173', origin: 'http://192.168.1.5:5173' })).toBe(
      false,
    );
    expect(fromDevPage({ host: 'evil.test:5173', origin: 'http://evil.test:5173' })).toBe(false);
    expect(fromDevPage({ host: 'evil.test@localhost:5173', 'sec-fetch-site': 'same-origin' })).toBe(
      false,
    );
    expect(fromDevPage({ origin: 'http://localhost:5173' })).toBe(false);
  });
});
