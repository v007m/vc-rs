import type { MessageShape } from '@bufbuild/protobuf';
import type * as E from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { click } from '../act.ts';
import type { Fakes } from '../engine.ts';
import type { State } from '../states.ts';

/** The engine answers the onboarding screens need (batch E). */

const schema = (kind: string) =>
  `# yaml-language-server: $schema=https://vibranium.dev/schema/v2/${kind}.schema.json\n`;

function config(project: string, profile: string, baseUrl: string, sample: boolean): string {
  const sampleVar = sample ? '  sampleUrl: http://localhost:7421    # the demo service\n' : '';
  const base = baseUrl ? `  baseUrl: ${baseUrl}\n` : '';
  const vars = sampleVar || base ? `vars:\n${sampleVar}${base}\n` : '';
  return `${schema('config')}# ─ Project config ─────────────────────────────────────────────
# One per project, at the root: profiles, project-wide vars, auth,
# secrets and run settings. Only syntaxVersion and one profile are required.
# ─────────────────────────────────────────────────────────────
syntaxVersion: 2
project: ${project}

defaults:
  timeout: 30s            # per request
  concurrency: 16         # test instances running at once
  headers:
    accept: application/json

${vars}profiles:
  - id: ${profile}
    default: true

secrets:
  provider: env            # environment and .env
  schema: .env.schema      # declares the secret names

reports:
  dir: .vibranium/reports
  formats:
    - ctrf
    - junit
`;
}

const serviceApi = (service: string) => `${schema('api')}id: ${service}
name: ${service[0]?.toUpperCase() ?? ''}${service.slice(1)} service
baseUrl: \${var.baseUrl}

apis:
  - id: list
    name: List ${service}
    method: GET
    path: /${service}
`;

const sampleApi = `${schema('api')}# Safe to delete once you have your own api files.
id: sample
name: Demo service built into Vibranium
baseUrl: \${var.sampleUrl}

apis:
  - id: listUsers
    name: List users
    method: GET
    path: /users

  - id: createUser
    name: Create a user
    method: POST
    path: /users
    vars:
      name: \${fake.person.fullName}
      email: \${fake.internet.email}
    body:
      name: \${var.name}
      email: \${var.email}
    expect:
      status: 201
    save:
      userId: \${response.body.id}
`;

const sampleTests = `${schema('test')}# Run this file: vc run sample.basics
id: sample.basics
name: Sample tests
tags:
  - sample

tests:
  - id: listsUsers
    name: Lists users
    call: sample#listUsers
    expect:
      status: 200

  - id: createsUser
    name: Creates a user with the given email
    call: sample#createUser
    vars:
      email: ada@example.com
    expect:
      body:
        match:
          email: ada@example.com
`;

const root = '~/dev/shop';

type Init = MessageShape<typeof E.InitRequestSchema>;

/** What `vc init` would write for these answers, as the engine previews it (D716). */
function initFiles(req: Pick<Init, 'name' | 'service' | 'baseUrl' | 'profile' | 'sample'>) {
  const files = [
    {
      path: 'vibranium.config.yaml',
      after: config(req.name || 'shop', req.profile || 'default', req.baseUrl ?? '', req.sample),
    },
    { path: '.env.schema', after: '# Secret names, one per line: NAME=description\n' },
    { path: '.gitignore', after: '.vibranium/\n.env\n' },
  ];
  if (req.service) {
    files.push({
      path: `services/${req.service}/${req.service}.api.yaml`,
      after: serviceApi(req.service),
    });
  }
  if (req.sample) {
    files.push(
      { path: 'services/sample/basics.test.yaml', after: sampleTests },
      { path: 'services/sample/sample.api.yaml', after: sampleApi },
    );
  }
  files.sort((a, b) => a.path.localeCompare(b.path));
  return { files, titles: files.map((f) => `Create ${f.path}`), digest: 'd-init', root };
}

export const fakes: Fakes = {
  init: (req) => initFiles(req),
  demo: () => ({
    project: initFiles({ name: '', service: '', baseUrl: '', profile: '', sample: true }),
    running: false,
    url: 'http://localhost:7421',
    start: 'vc demo start --detach',
  }),
};

/** The folder `vc ui` serves has no config yet, so the stage shows onboarding. */
const unconfigured: Fakes = {
  getProject: () => ({
    name: '',
    profiles: [],
    vcVersion: '2.0.0-dev',
    auths: [],
    configured: false,
  }),
};

/** The onboarding screens and overlays the audit checks. */
export const states: State[] = [
  { name: 'onboarding', path: '/client', fakes: unconfigured },
  {
    name: 'onboarding-init-preview',
    path: '/client',
    fakes: unconfigured,
    act: async (page) => {
      await page.getByRole('textbox', { name: 'Project name' }).fill('shop');
      await page.getByRole('textbox', { name: 'First service' }).fill('users');
      await page.getByRole('textbox', { name: 'Base URL' }).fill('http://localhost:8080');
      await click(page, 'Preview');
      await page.getByRole('button', { name: 'Write files' }).waitFor();
    },
  },
  {
    name: 'onboarding-demo',
    path: '/client',
    fakes: unconfigured,
    act: async (page) => {
      await click(page, 'Try the demo');
      await click(page, 'Preview the demo');
      await page.getByText('vc demo start --detach').waitFor();
    },
  },
];
