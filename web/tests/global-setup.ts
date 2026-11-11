import { fileURLToPath } from 'node:url';
import { chromium, type FullConfig } from '@playwright/test';
import { startVcUi } from './vcui';

/** Where the session cookie is saved; playwright.config.ts loads it for every test. */
const storageState = fileURLToPath(new URL('.auth/state.json', import.meta.url));

/**
 * Starts `vc ui --no-open` on a copy of the seed project (the editor spec saves files), opens the
 * launch link once so the server sets its session cookie, and saves that cookie for the tests.
 * The used link and the agent token go to the specs as VC_E2E_LAUNCH_URL and VC_E2E_AGENT_TOKEN.
 * Returns the teardown that stops vc ui.
 */
export default async function globalSetup(config: FullConfig): Promise<() => Promise<void>> {
  const baseURL = config.projects[0]?.use.baseURL ?? 'http://127.0.0.1:7420';
  const port = new URL(baseURL).port;
  const { printed, stop } = await startVcUi(port);
  try {
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
