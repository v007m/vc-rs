import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { HarImport, maxHarBytes } from './har.svelte.ts';
import type { HarInput } from './port.ts';

const changes = {
  files: [{ path: 'services/users/recorded.test.yaml', before: '', after: 'id: users.recorded\n' }],
  titles: ['Add users.recorded#replaysSession'],
};

function setup(fail = false) {
  const sent: HarInput[] = [];
  const har = new HarImport({
    importHar: (input) => {
      sent.push(input);
      return fail
        ? Promise.reject(new ConnectError('not a HAR file', Code.InvalidArgument))
        : Promise.resolve(changes);
    },
  });
  return { har, sent };
}

const file = new File(['{"log":{}}'], 'session.har');

describe('HarImport', () => {
  it('previews, then writes the same import', async () => {
    const { har, sent } = setup();
    await har.choose(file);
    har.setFlow(' users.recorded#replaysSession ');
    await har.showPreview();
    expect(har.preview).toEqual(changes);
    await har.write();
    expect(har.written).toEqual(['Add users.recorded#replaysSession']);
    expect(sent).toEqual([
      { source: '{"log":{}}', flow: 'users.recorded#replaysSession', write: false },
      { source: '{"log":{}}', flow: 'users.recorded#replaysSession', write: true },
    ]);
  });

  it('drops the preview when the flow changes and never writes without one', async () => {
    const { har, sent } = setup();
    await har.choose(file);
    await har.showPreview();
    har.setFlow('x');
    await har.write();
    expect(har.preview).toBeUndefined();
    expect(sent).toHaveLength(1);
  });

  it('refuses a file the server could not take whole and shows engine errors', async () => {
    const { har } = setup(true);
    await har.choose(file);
    expect(har.fileName).toBe('session.har');
    await har.choose({ name: 'big.har', size: maxHarBytes + 1 } as File);
    expect(maxHarBytes).toBeLessThan(4 * 1024 * 1024);
    expect(har.error).toBe('big.har is too large (max 3.9 MB).');
    expect(har.fileName).toBeUndefined();
    expect(har.ready).toBe(false);
    await har.choose(file);
    await har.showPreview();
    expect(har.error).toBe('not a HAR file');
  });
});
