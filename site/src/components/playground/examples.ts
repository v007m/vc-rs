import config from '~/examples/vibranium.config.yaml?raw';
import smokeLoad from '~/examples/load/smoke.load.yaml?raw';
import authTest from '~/examples/services/echo/auth.test.yaml?raw';
import searchTest from '~/examples/services/echo/search.test.yaml?raw';
import crudTest from '~/examples/services/users/crud.test.yaml?raw';
import pollingTest from '~/examples/services/users/polling.test.yaml?raw';
import signupTest from '~/examples/services/users/signup.test.yaml?raw';
import usersApi from '~/examples/services/users/users.api.yaml?raw';
import type { Kind } from './schemas';

export interface Example {
  id: string;
  label: string;
  kind: Kind;
  file: string;
  blurb: string;
  code: string;
}

/** Every example is a file of one real project (site/src/examples) that passes `vc check` and `vc run`. */
export const examples: Example[] = [
  {
    id: 'crud',
    label: 'CRUD test',
    kind: 'test',
    file: 'services/users/crud.test.yaml',
    blurb: 'Create, read and reject users. The api file declares the cleanup, so nothing is left behind.',
    code: crudTest,
  },
  {
    id: 'api',
    label: 'Api file',
    kind: 'api',
    file: 'services/users/users.api.yaml',
    blurb: 'How to call the users service: defaults, checks, saved values and an automatic cleanup.',
    code: usersApi,
  },
  {
    id: 'examples',
    label: 'Data-driven',
    kind: 'test',
    file: 'services/users/signup.test.yaml',
    blurb: 'One test instance per `examples` row, with each row read as `${example.*}`.',
    code: signupTest,
  },
  {
    id: 'query',
    label: 'Query params',
    kind: 'test',
    file: 'services/echo/search.test.yaml',
    blurb: 'Rows feed query parameters; a pipe turns a number into the text the server echoes back.',
    code: searchTest,
  },
  {
    id: 'polling',
    label: 'Polling',
    kind: 'test',
    file: 'services/users/polling.test.yaml',
    blurb: 'Re-send a request until a condition holds, then check `expect`. `until`, `every` and `timeout` are all required.',
    code: pollingTest,
  },
  {
    id: 'auth',
    label: 'Auth flow',
    kind: 'test',
    file: 'services/echo/auth.test.yaml',
    blurb: 'Uses the `demoLogin` flow from the config example, once as the default user and once with other vars.',
    code: authTest,
  },
  {
    id: 'config',
    label: 'Config + profiles',
    kind: 'config',
    file: 'vibranium.config.yaml',
    blurb: 'Two profiles that switch base URLs, and a multi-step `flow` login that the auth example uses.',
    code: config,
  },
  {
    id: 'load',
    label: 'Load file',
    kind: 'load',
    file: 'load/smoke.load.yaml',
    blurb: 'Turns two existing tests into a weighted traffic mix with stages, think time and thresholds.',
    code: smokeLoad,
  },
];
