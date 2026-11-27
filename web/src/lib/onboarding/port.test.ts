import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { folderName, initNotYet, onboardingPort, type InitChoices } from './port.ts';

const blank: InitChoices = { name: '', service: '', baseUrl: '', profile: '', sample: true };
const config = { path: 'vibranium.config.yaml', after: 'name: shop\n' };

describe('onboardingPort', () => {
  it('previews with only the answers given, then writes with the digest', async () => {
    const asked: Record<string, unknown>[] = [];
    const port = onboardingPort(
      fakeEngine({
        init: ({ name, service, baseUrl, profile, sample, write, expectedDigest }) => {
          asked.push({ name, service, baseUrl, profile, sample, write, expectedDigest });
          return { files: [config], digest: 'd1', root: '~/dev/shop' };
        },
      }),
    );
    const dry = await port.init({ ...blank, service: ' users ', sample: false });
    expect(dry).toEqual({
      available: true,
      value: {
        files: [{ path: 'vibranium.config.yaml', before: '', after: 'name: shop\n' }],
        digest: 'd1',
        root: '~/dev/shop',
      },
    });
    await port.init(blank, 'd1');
    const unset = { name: undefined, baseUrl: undefined, profile: undefined };
    expect(asked).toEqual([
      { ...unset, service: 'users', sample: false, write: false, expectedDigest: undefined },
      { ...unset, service: undefined, sample: true, write: true, expectedDigest: 'd1' },
    ]);
  });

  it('previews and writes the demo, keeping its service facts', async () => {
    const asked: { write: boolean; expectedDigest?: string | undefined }[] = [];
    const port = onboardingPort(
      fakeEngine({
        demo: ({ write, expectedDigest }) => {
          asked.push({ write, expectedDigest });
          return {
            project: { files: [config], digest: 'd2', root: '~/demo' },
            running: false,
            url: 'http://localhost:7421',
            start: 'vc demo start --detach',
          };
        },
      }),
    );
    const got = await port.demo();
    expect(got.available && got.value).toMatchObject({
      digest: 'd2',
      root: '~/demo',
      running: false,
      url: 'http://localhost:7421',
      start: 'vc demo start --detach',
    });
    await port.demo('d2');
    expect(asked).toEqual([
      { write: false, expectedDigest: undefined },
      { write: true, expectedDigest: 'd2' },
    ]);
  });

  it('answers a demo with no project files as an empty preview', async () => {
    const port = onboardingPort(
      fakeEngine({ demo: () => ({ running: true, url: 'http://localhost:7421', start: 'x' }) }),
    );
    const got = await port.demo();
    expect(got.available && got.value).toMatchObject({ files: [], digest: '', running: true });
  });

  it('says to run vc init in a terminal when the engine has no Init or Demo', async () => {
    const port = onboardingPort(fakeEngine({}));
    expect(await port.init(blank)).toEqual({ available: false, reason: initNotYet });
    expect(await port.demo()).toEqual({ available: false, reason: initNotYet });
    expect(initNotYet).toContain('vc init');
  });

  it('names the last folder of a root', () => {
    expect(folderName('~/dev/shop')).toBe('shop');
    expect(folderName('/srv/api/')).toBe('api');
    expect(folderName('~')).toBe('~');
  });
});
