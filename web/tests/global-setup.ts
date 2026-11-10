import { spawn, type ChildProcess } from 'node:child_process';
import { cp, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, type FullConfig } from '@playwright/test';

/** Where the session cookie is saved; playwright.config.ts loads it for every test. */
const storageState = fileURLToPath(new URL('.auth/state.json', import.meta.url));

const seedDir = fileURLToPath(new URL('fixtures/seed', import.meta.url));
const launchLine = /vc ui: (http:\/\/\S+\?token=[A-Za-z0-9_-]{43})/;
const agentLine = /vc ui: mcp (http:\/\/\S+\/mcp) token (\S+)/;
const startTimeoutMs = 30_000;

/** What `vc ui` prints: the one-time launch link, and where and with what token an agent connects. */
type Printed = Readonly<{ launchUrl: string; mcpUrl: string; agentToken: string }>;

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

/**
 * Starts `vc ui --no-open` on a copy of the seed project (the editor spec saves files), opens the
 * launch link once so the server sets its session cookie, and saves that cookie for the tests.
 * The used link and the agent token go to the specs as VC_E2E_LAUNCH_URL and VC_E2E_AGENT_TOKEN.
 * Returns the teardown that stops vc ui. VC_BIN points at a vc other than the one on PATH, such
 * as target/debug/vc.
 */
export default async function globalSetup(config: FullConfig): Promise<() => Promise<void>> {
  const baseURL = config.projects[0]?.use.baseURL ?? 'http://127.0.0.1:7420';
  const port = new URL(baseURL).port;
  const vc = process.env.VC_BIN ?? 'vc';
  const projectDir = await mkdtemp(join(tmpdir(), 'vc-e2e-'));
  await cp(seedDir, projectDir, { recursive: true });
  const child = spawn(vc, ['ui', '--no-open', '--port', port, '--profile', 'dev'], {
    cwd: projectDir,
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const stop = async (): Promise<void> => {
    child.kill();
    await rm(projectDir, { recursive: true, force: true });
  };
  try {
    const printed = await readPrinted(child);
    process.env.VC_E2E_LAUNCH_URL = printed.launchUrl;
    process.env.VC_E2E_MCP_URL = printed.mcpUrl;
    process.env.VC_E2E_AGENT_TOKEN = printed.agentToken;
    const browser = await chromium.launch();
    const page = await browser.newPage();
    await page.goto(printed.launchUrl);
    await page.context().storageState({ path: storageState });
    await browser.close();
  } catch (error) {
    await stop();
    throw error;
  }
  return stop;
}
