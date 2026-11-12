import { spawn, type ChildProcess } from 'node:child_process';
import { cp, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const seedDir = fileURLToPath(new URL('fixtures/seed', import.meta.url));
const launchLine = /vc ui: (http:\/\/\S+\?token=[A-Za-z0-9_-]{43})/;
const agentLine = /vc ui: mcp (http:\/\/\S+\/mcp) token (\S+)/;
const startTimeoutMs = 30_000;
const killTimeoutMs = 5_000;

/** What `vc ui` prints: the one-time launch link, and where and with what token an agent connects. */
export type Printed = Readonly<{ launchUrl: string; mcpUrl: string; agentToken: string }>;

/**
 * A running `vc ui` over its own copy of the seed project, and how to stop it. `origin` is where
 * it listens, read from the launch link, so a `vc ui` on port 0 is reached on the port it was given.
 */
export type VcUi = Readonly<{
  printed: Printed;
  origin: string;
  projectDir: string;
  stop: () => Promise<void>;
}>;

/** Waits for the two lines `vc ui` prints, `vc ui: <url>?token=…` and `vc ui: mcp <url> token …`. */
function readPrinted(child: ChildProcess): Promise<Printed> {
  return new Promise((resolve, reject) => {
    let output = '';
    const timer = setTimeout(
      () => reject(new Error(`vc ui printed no launch and mcp lines:\n${output}`)),
      startTimeoutMs,
    );
    child.stdout?.on('data', (chunk: Buffer) => {
      output += chunk.toString();
      const launchUrl = launchLine.exec(output)?.[1];
      const [, mcpUrl, agentToken] = agentLine.exec(output) ?? [];
      if (launchUrl && mcpUrl && agentToken) {
        clearTimeout(timer);
        resolve({ launchUrl, mcpUrl, agentToken });
      }
    });
    child.once('exit', (code) => reject(new Error(`vc ui exited with ${code}:\n${output}`)));
  });
}

/** Resolves when `child` has exited, at once if it already has. */
function exited(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise((resolve) => child.once('exit', () => resolve()));
}

/**
 * Ends `child` and waits for it to exit, so its port is free for the next `vc ui`: SIGTERM first,
 * then SIGKILL if it has not exited after `killTimeoutMs`.
 */
async function end(child: ChildProcess): Promise<void> {
  const done = exited(child);
  child.kill();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const late = new Promise<boolean>((resolve) => {
    timer = setTimeout(() => resolve(true), killTimeoutMs);
  });
  const timedOut = await Promise.race([done.then(() => false), late]);
  clearTimeout(timer);
  if (timedOut) {
    child.kill('SIGKILL');
    await done;
  }
}

/**
 * Starts `vc ui --no-open` on `port` over a temporary copy of the seed project, which `prepare`
 * may change first, and waits for its launch line. Port '0' lets the OS pick a free port, so specs
 * never clash with each other or with a `vc ui` already running. VC_BIN points at a vc other than
 * the one on PATH, such as target/debug/vc. `stop` ends vc ui, waits for it to exit, then deletes
 * the copy.
 */
export async function startVcUi(
  port: string,
  prepare: (projectDir: string) => Promise<void> = () => Promise.resolve(),
): Promise<VcUi> {
  const projectDir = await mkdtemp(join(tmpdir(), 'vc-e2e-'));
  await cp(seedDir, projectDir, { recursive: true });
  await prepare(projectDir);
  const child = spawn(
    process.env.VC_BIN ?? 'vc',
    ['ui', '--no-open', '--port', port, '--profile', 'dev'],
    {
      cwd: projectDir,
      stdio: ['ignore', 'pipe', 'inherit'],
    },
  );
  const stop = async (): Promise<void> => {
    await end(child);
    await rm(projectDir, { recursive: true, force: true });
  };
  try {
    const printed = await readPrinted(child);
    return { printed, origin: new URL(printed.launchUrl).origin, projectDir, stop };
  } catch (error) {
    await stop();
    throw error;
  }
}
