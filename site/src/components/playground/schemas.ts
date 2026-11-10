import api from '../../../../schema/api.schema.json';
import config from '../../../../schema/config.schema.json';
import load from '../../../../schema/load.schema.json';
import test from '../../../../schema/test.schema.json';
import type { Kind } from '~/lib/schemas';

export type { Kind };

export const schemas: Record<Kind, Record<string, unknown>> = { test, api, config, load };

export const fileNames: Record<Kind, string> = {
  test: 'services/users/crud.test.yaml',
  api: 'services/users/users.api.yaml',
  config: 'vibranium.config.yaml',
  load: 'load/smoke.load.yaml',
};
