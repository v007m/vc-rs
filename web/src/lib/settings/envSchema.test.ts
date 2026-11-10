import { describe, expect, it } from 'vitest';
import { declare, nameProblem, parseSchema, undeclare } from './envSchema.ts';

const schema = [
  '# Secrets for the shop api',
  '# ---',
  '',
  '# The token the checkout tests use',
  '# @sensitive @type=string',
  'SHOP_TOKEN=',
  'export REGION=eu-west-1',
  '',
  '# @sensitive',
  'HOOK_URL=https://never.shown/hook',
  'SHOP_TOKEN=again',
  '',
].join('\n');

describe('.env.schema', () => {
  it('reads each declared name with its annotations and notes, never its value', () => {
    const entries = parseSchema(schema);
    expect(entries).toEqual([
      {
        name: 'SHOP_TOKEN',
        annotations: ['@sensitive', '@type=string'],
        notes: ['The token the checkout tests use'],
      },
      { name: 'REGION', annotations: [], notes: [] },
      { name: 'HOOK_URL', annotations: ['@sensitive'], notes: [] },
    ]);
    expect(JSON.stringify(entries)).not.toContain('never.shown');
  });

  it('reads names exactly as the engine does', () => {
    const names = (text: string) => parseSchema(text).map((entry) => entry.name);
    expect(names('export\tTAB=\nexport =1\nexport  TWO=\n  SPACED = x\n # HIDDEN=\n')).toEqual([
      'TWO',
      'SPACED',
    ]);
    expect(names('_X1 = 2\n1BAD=x\nno equals\nA-B=1\nexport=1\nlower=\n')).toEqual([
      '_X1',
      'export',
      'lower',
    ]);
  });

  it('keeps a header above a divider out of the first name', () => {
    expect(parseSchema('# header\n# ---\n# @sensitive\nA=\n')[0]?.notes).toEqual([]);
  });

  it('refuses a name that breaks the convention or is declared already', () => {
    const entries = parseSchema(schema);
    expect(nameProblem('PAY_KEY', entries)).toBeUndefined();
    expect(nameProblem('payKey', entries)).toBeUndefined();
    expect(nameProblem('_X1', entries)).toBeUndefined();
    expect(nameProblem('1KEY', entries)).toMatch(/not starting with a digit/);
    expect(nameProblem('A-B', entries)).toMatch(/not starting with a digit/);
    expect(nameProblem('REGION', entries)).toBe('REGION is declared already.');
  });

  it('declares a name on a line of its own at the end', () => {
    expect(declare('', 'A')).toBe('A=\n');
    expect(declare('A=', 'B')).toBe('A=\nB=\n');
    expect(declare('A=\n', 'B')).toBe('A=\nB=\n');
  });

  it('removes every line declaring a name with its comment block, and nothing else', () => {
    const text = undeclare(schema, 'SHOP_TOKEN');
    expect(text).toBe(
      [
        '# Secrets for the shop api',
        '# ---',
        '',
        'export REGION=eu-west-1',
        '',
        '# @sensitive',
        'HOOK_URL=https://never.shown/hook',
        '',
      ].join('\n'),
    );
    expect(undeclare(schema, 'MISSING')).toBe(schema);
  });
});
