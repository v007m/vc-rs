// Takes the docs screenshots of `vc run`, the TUI, the HTML report and the web UI, in a light and a
// dark theme, from a scratch copy of the walkthrough project run against `vc demo`.
//
//   mise run screenshots                           builds vc, vc-demo and the web UI, then runs this
//   VC_BIN=<path to vc> node scripts/screenshots.mjs [cli|tui|report|web ...]
//
// Pictures go to src/assets/screenshots/<area>/<name>-<theme>.webp. The TUI paints its own colours
// whatever the terminal's theme, so its pictures come only as `-dark`.
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';
import sharp from 'sharp';
import { captureReport, captureWebUi } from './screenshots/browser.mjs';
import { fixHistory } from './screenshots/history.mjs';
import { DEMO_PORT, createProject, startDemo, vc } from './screenshots/project.mjs';
import { consoleRun, renderTerminals, tuiSessions } from './screenshots/terminals.mjs';

const site = join(dirname(fileURLToPath(import.meta.url)), '..');
const vcBin = process.env.VC_BIN ?? join(site, '..', 'target/debug/vc');
const outDir = join(site, 'src/assets/screenshots');
const AREAS = ['cli', 'tui', 'report', 'web'];
const wanted = new Set(process.argv.length > 2 ? process.argv.slice(2) : AREAS);

/** Writes one picture as WebP; the same pixels give the same bytes, so an unchanged screen is no diff. */
async function save(png, area, name) {
  if (!wanted.has(area)) return;
  mkdirSync(join(outDir, area), { recursive: true });
  await sharp(png).webp({ quality: 80, effort: 6, smartSubsample: true }).toFile(join(outDir, area, `${name}.webp`));
  console.log(`  ${area}/${name}.webp`);
}

/** Runs `step` with a fresh demo, so its users and counters start from nothing each time. */
async function withDemo(step) {
  const demo = await startDemo(vcBin);
  try {
    return await step();
  } finally {
    await demo.stop();
  }
}

async function main() {
  for (const area of wanted) if (!AREAS.includes(area)) throw new Error(`unknown area ${area}; choose from ${AREAS.join(', ')}`);
  if (!existsSync(vcBin)) throw new Error(`no vc binary at ${vcBin}; run \`mise run screenshots\` or set VC_BIN`);
  const { root, project } = createProject({ examples: join(site, 'src/examples'), fixture: join(site, 'scripts/screenshots/fixture') });
  const browser = await chromium.launch();
  try {
    // Run 1 is the `vc run` picture, run 2 retries its failures, runs 3 and 4 come from the TUI.
    console.log('run 1: vc run');
    const cli = await withDemo(() => consoleRun({ vcBin, project }));
    console.log('run 2: vc run --failed');
    await withDemo(() => vc(vcBin, project, ['run', '--failed', '--seed', '42']));
    const specUrl = `http://localhost:${DEMO_PORT}/openapi/users.yaml`;
    console.log('runs 3 and 4: vc tui');
    const tui = await withDemo(() => tuiSessions({ vcBin, project, specUrl, fixHistory: () => fixHistory(project) }));
    const shoot = { browser, save };
    await renderTerminals(shoot, { cli, tui });
    if (wanted.has('report')) await captureReport(shoot, { vcBin, project, root });
    if (wanted.has('web')) await withDemo(() => captureWebUi(shoot, { vcBin, project }));
  } finally {
    await browser.close();
    rmSync(root, { recursive: true, force: true });
  }
}

await main();
