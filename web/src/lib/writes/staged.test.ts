import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { available, notYet } from '#lib/api/maybe.ts';
import { preview, Staged } from './staged.svelte.ts';

const shown = { files: [{ path: 'a.yaml', before: '', after: 'x\n' }], digest: 'd1' };

describe('Staged', () => {
  it('keeps the dry run, then writes with its digest', async () => {
    const staged = new Staged();
    await staged.review(async () => available(shown));
    expect(staged.shown).toEqual(shown);
    let sent = '';
    const wrote = await staged.write(async (digest) => available((sent = digest)));
    expect([wrote, sent, staged.shown]).toEqual([true, 'd1', undefined]);
  });

  it('writes nothing before a preview', async () => {
    expect(await new Staged().write(async () => available(1))).toBe(false);
  });

  it('notes why the engine has no answer, and the error a step throws', async () => {
    const staged = new Staged();
    await staged.review(async () => notYet('Needs a newer vc.'));
    expect(staged.note).toBe('Needs a newer vc.');
    await staged.review(async () => {
      throw new ConnectError('stale digest', Code.FailedPrecondition);
    });
    expect([staged.note, staged.busy]).toEqual(['stale digest', false]);
  });

  it('turns a wire dry run into a preview, a new file empty before', () => {
    const got = preview({ files: [{ path: 'b.yaml', after: 'y' }] as never, digest: 'd2' });
    expect(got).toEqual({ files: [{ path: 'b.yaml', before: '', after: 'y' }], digest: 'd2' });
  });
});
