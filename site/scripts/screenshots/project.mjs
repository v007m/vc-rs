// The scratch project the screenshots are taken in: the walkthrough examples plus a few tests that
// fail, block, retry and skip, run against `vc demo`.
import { execFileSync } from 'node:child_process';
import { cpSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { sleep } from './terminal.mjs';

export const DEMO_PORT = 7491;
/** When the first stored run starts: 2026-11-02 09:30 UTC. */
export const FIRST_RUN_AT = Date.UTC(2026, 10, 2, 9, 30);
/** The environment every `vc` process runs with, so dates and colours do not depend on the machine. */
export function vcEnv() {
  const env = { ...process.env, TZ: 'UTC', LANG: 'en_US.UTF-8', LC_ALL: 'en_US.UTF-8' };
  delete env.NO_COLOR;
  return env;
}

/**
 * Copies the walkthrough project and the fixture to a fixed folder outside the site (a `.vibranium`
 * folder inside `site/src/examples` would break the playground tests), points it at the demo's port,
 * runs one test at a time and commits it with a fixed date, so the commit hash is the same each time.
 */
export function createProject({ examples, fixture }) {
  const root = join(tmpdir(), 'vc-screenshots');
  const project = join(root, 'shop');
  rmSync(root, { recursive: true, force: true });
  cpSync(examples, project, { recursive: true, filter: (source) => !source.includes('.vibranium') });
  cpSync(fixture, project, { recursive: true });
  for (const file of yamlFiles(project)) {
    const text = readFileSync(file, 'utf8').replaceAll('localhost:7421', `localhost:${DEMO_PORT}`);
    writeFileSync(file, text);
  }
  edit(join(project, 'vibranium.config.yaml'), /concurrency: \d+/, 'concurrency: 1');
  // The script starts and stops the demo itself; the file's own `vc demo start` would join it.
  edit(join(project, 'services/users/crud.test.yaml'), /^beforeAll:\n(?: .*\n)+afterAll:\n(?: .*\n)+\n/m, '');
  commit(project);
  return { root, project };
}

function edit(file, pattern, replacement) {
  const text = readFileSync(file, 'utf8');
  if (!pattern.test(text)) throw new Error(`${file} no longer matches ${pattern}`);
  writeFileSync(file, text.replace(pattern, replacement));
}

function yamlFiles(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return yamlFiles(path);
    return name.endsWith('.yaml') ? [path] : [];
  });
}

function commit(project) {
  const date = new Date(FIRST_RUN_AT - 3_600_000).toISOString();
  const env = {
    ...process.env,
    GIT_AUTHOR_NAME: 'Docs',
    GIT_AUTHOR_EMAIL: 'docs@example.com',
    GIT_COMMITTER_NAME: 'Docs',
    GIT_COMMITTER_EMAIL: 'docs@example.com',
    GIT_AUTHOR_DATE: date,
    GIT_COMMITTER_DATE: date,
  };
  const git = (...args) => execFileSync('git', args, { cwd: project, env, stdio: 'ignore' });
  git('init', '-q', '-b', 'main');
  writeFileSync(join(project, '.gitignore'), '.vibranium/\n');
  git('add', '-A');
  git('-c', 'commit.gpgsign=false', 'commit', '-q', '-m', 'Add the shop tests');
}

async function answers() {
  try {
    const response = await fetch(`http://127.0.0.1:${DEMO_PORT}/echo`);
    return response.ok;
  } catch {
    return false;
  }
}

/** Starts a fresh `vc demo`, so its users and counters start from nothing, and returns its `stop`. */
export async function startDemo(vcBin) {
  if (await answers()) throw new Error(`something already answers on port ${DEMO_PORT}; stop it first`);
  const port = String(DEMO_PORT);
  execFileSync(vcBin, ['demo', 'start', '--port', port, '--detach'], { env: vcEnv(), stdio: 'ignore' });
  return {
    async stop() {
      execFileSync(vcBin, ['demo', 'stop', '--port', port], { env: vcEnv(), stdio: 'ignore' });
      for (let i = 0; await answers(); i++) {
        if (i > 50) throw new Error(`vc demo stop left port ${DEMO_PORT} answering`);
        await sleep(100);
      }
    },
  };
}

/** Runs `vc` in the project and returns its output; a failing run is expected. */
export function vc(vcBin, project, args) {
  try {
    return execFileSync(vcBin, args, { cwd: project, env: vcEnv(), encoding: 'utf8' });
  } catch (error) {
    if (typeof error.status !== 'number' || error.status > 4) throw error;
    return error.stdout;
  }
}
