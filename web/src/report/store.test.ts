import { gzipSync } from 'node:zlib';
import { describe, expect, it } from 'vitest';
import { DataStore, inflate } from './store.ts';

describe('DataStore', () => {
  it('asks for a key once and answers everyone waiting when its file loads', async () => {
    const asked: string[] = [];
    const store = new DataStore((key) => asked.push(key));
    const first = store.get('summary');
    const second = store.get('summary');
    store.load('summary', { run: 1 });
    expect(await first).toEqual({ run: 1 });
    expect(await second).toEqual({ run: 1 });
    expect(await store.get('summary')).toEqual({ run: 1 });
    expect(asked).toEqual(['summary']);
  });

  it('fails a key whose file does not load, and asks again next time', async () => {
    const asked: string[] = [];
    const store = new DataStore((key, failed) => {
      asked.push(key);
      failed();
    });
    await expect(store.get('tests/3')).rejects.toThrow("The report's data/tests/3.js");
    await expect(store.get('tests/3')).rejects.toThrow();
    expect(asked).toEqual(['tests/3', 'tests/3']);
  });

  it('serves every key from a bundle, and only the keys it has', async () => {
    const store = new DataStore(() => {
      throw new Error('a bundled report requests nothing');
    });
    store.useBundle(Promise.resolve({ index: [], 'bodies/ab': 'text' }));
    expect(await store.get('bodies/ab')).toBe('text');
    await expect(store.get('toString')).rejects.toThrow('The report has no toString.');
  });
});

describe('inflate', () => {
  it('reads base64 gzip JSON, as the single-file layout embeds it', async () => {
    const data = { summary: { run: { id: 7 } }, 'bodies/x': '</script>' };
    const base64 = gzipSync(JSON.stringify(data)).toString('base64');
    expect(await inflate(`\n${base64}\n`)).toEqual(data);
  });
});
