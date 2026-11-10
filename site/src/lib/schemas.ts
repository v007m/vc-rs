import apiRaw from '../../../schema/api.schema.json?raw';
import configRaw from '../../../schema/config.schema.json?raw';
import loadRaw from '../../../schema/load.schema.json?raw';
import testRaw from '../../../schema/test.schema.json?raw';

export type Kind = 'test' | 'api' | 'config' | 'load';

export const KINDS: Kind[] = ['config', 'api', 'test', 'load'];

/** The exact bytes of each file in the repo-root `schema/` folder, the single source (D258). */
export const schemaText: Record<Kind, string> = { api: apiRaw, config: configRaw, load: loadRaw, test: testRaw };

export const SCHEMA_BASE = `${(import.meta.env.SITE ?? '').replace(/\/$/, '')}${import.meta.env.BASE_URL.replace(/\/$/, '')}/schema/`;

export const schemaUrl = (kind: Kind): string => `${SCHEMA_BASE}${kind}.schema.json`;

export const fileMatch: Record<Kind, string[]> = {
  config: ['vibranium.config.yaml'],
  api: ['*.api.yaml'],
  test: ['*.test.yaml'],
  load: ['*.load.yaml'],
};

export const schemaDescription: Record<Kind, string> = {
  config: 'Vibranium project config (vibranium.config.yaml)',
  api: 'Vibranium api file (*.api.yaml)',
  test: 'Vibranium test file (*.test.yaml)',
  load: 'Vibranium load scenario (*.load.yaml)',
};
