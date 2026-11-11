import { describe, expect, it } from 'vitest';
import { rawVars, scalarText } from './rawVars.ts';

const CONFIG = `# vibranium.config.yaml
project: shop
vars:
  # The users service.
  usersUrl: \${env.USERS_URL}/v1  # per stack
  quoted: 'it''s \${var.a}'
  doubled: "say \\"hi\\" \\\\ \${var.b}"
  nested:
    deeper: 1
  flow: {a: 1}
  block: |
    text
  odd: "tab\\t"

profiles:
  - id: dev
    vars:
      usersUrl: http://localhost
`;

describe('rawVars', () => {
  it('reads one-line scalars of the top-level vars block as written', () => {
    expect(Object.fromEntries(rawVars(CONFIG))).toEqual({
      usersUrl: '${env.USERS_URL}/v1',
      quoted: "it's ${var.a}",
      doubled: 'say "hi" \\ ${var.b}',
    });
  });

  it('reads nothing from a file with no top-level vars block', () => {
    expect(rawVars('project: shop\nprofiles:\n  - id: dev\n    vars:\n      a: b\n').size).toBe(0);
  });

  it('leaves out what it cannot read for certain', () => {
    expect(scalarText('[1, 2]')).toBeUndefined();
    expect(scalarText('&anchor x')).toBeUndefined();
    expect(scalarText("'open")).toBeUndefined();
    expect(scalarText('plain # note')).toBe('plain');
    expect(scalarText('a#b')).toBe('a#b');
  });
});
