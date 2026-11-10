import { spawn, type ChildProcess } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { chromium, type FullConfig } from '@playwright/test';

/** Where the session cookie is saved; playwright.config.ts loads it for every test. */
const storageState = fileURLToPath(new URL('.auth/state.json', import.meta.url));

const seedDir = fileURLToPath(new URL('fixtures/seed', import.meta.url));
const tokenUrl = /http:\/\/\S+\?token=[A-Za-z0-9_-]{43}/;
const startTimeoutMs = 30_000;

/** Waits for the one line `vc ui` prints, `vc ui: <url>?token=<token>`, and returns the URL. */
function readTokenUrl(child: ChildProcess): Promise<string> {
  return new Promise((resolve, reject) => {
    let output = '';
    const timer = setTimeout(
      () => reject(new Error(`vc ui printed no URL:\n${output}`)),
      startTimeoutMs,
    );
    child.stdout?.on('data', (chunk: Buffer) => {
      output += chunk.toString();
      const url = tokenUrl.exec(output)?.[0];
      if (url) {
        clearTimeout(timer);
        resolve(url);
      }
    });
    child.once('exit', (code) => reject(new Error(`vc ui exited with ${code}:\n${output}`)));
  });
}

/**
 * Starts `vc ui --no-open` on the seed project, opens the token URL once so the server sets
 * its session cookie, and saves that cookie for the tests. Returns the teardown that stops vc ui.
 * VC_BIN points at a vc other than the one on PATH, such as target/debug/vc.
 */
export default async function globalSetup(config: FullConfig): Promise<() => Promise<void>> {
  const baseURL = config.projects[0]?.use.baseURL ?? 'http://127.0.0.1:7420';
  const port = new URL(baseURL).port;
  const vc = process.env.VC_BIN ?? 'vc';
  const child = spawn(vc, ['ui', '--no-open', '--port', port, '--profile', 'dev'], {
    cwd: seedDir,
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  const stop = async (): Promise<void> => {
    child.kill();
  };
  try {
    const url = await readTokenUrl(child);
    const browser = await chromium.launch();
    const page = await browser.newPage();
    await page.goto(url);
    await page.context().storageState({ path: storageState });
    await browser.close();
  } catch (error) {
    await stop();
    throw error;
  }
  return stop;
}
