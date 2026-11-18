// The browser pictures: the HTML report `vc report` writes and the web UI `vc ui` serves, each in
// the light and the dark theme, which both pick from `prefers-color-scheme`.
import { spawn } from 'node:child_process';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { FIRST_RUN_AT, vc, vcEnv } from './project.mjs';

const THEMES = ['light', 'dark'];
const VIEWPORT = { width: 1280, height: 800 };
const WIDE = { width: 1440, height: 800 };
/**
 * The report's failure picture: tall enough that the test list (`100dvh - 260px`) holds all its rows
 * unscrolled, and wide enough that the detail pane keeps the error excerpt's underline on one line.
 */
const REPORT_FAILURE = { width: 1520, height: 1060 };
/** "Now" for relative dates: the morning the stored runs happened. */
const NOW = FIRST_RUN_AT + 4 * 3_600_000;

async function newPage(browser) {
  const context = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 2, locale: 'en-US', timezoneId: 'UTC', reducedMotion: 'reduce' });
  const page = await context.newPage();
  await page.clock.setFixedTime(NOW);
  return page;
}

/** Takes the page in each theme; `prepare` brings it to the state to show. */
async function bothThemes(page, save, area, name, prepare = async () => {}, clip = undefined) {
  for (const colorScheme of THEMES) {
    await page.emulateMedia({ colorScheme });
    await prepare();
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(300);
    await save(await page.screenshot({ animations: 'disabled', caret: 'hide', clip }), area, `${name}-${colorScheme}`);
  }
}

/** The single-file report of the last test run: its summary, then a failing test opened. */
export async function captureReport({ browser, save }, { vcBin, project, root }) {
  const out = join(root, 'report');
  vc(vcBin, project, ['report', '3', '--single-file', '--out', out]);
  const page = await newPage(browser);
  try {
    await page.goto(pathToFileURL(join(out, 'report.html')).href);
    await page.getByText('passed').first().waitFor();
    await bothThemes(page, save, 'report', 'summary');
    await page.setViewportSize(REPORT_FAILURE);
    await page.getByText('users.profile#getsAdaByName').first().click();
    await page.getByText('Ada Lovelace').first().waitFor();
    await page.mouse.move(0, 0);
    await page.evaluate(() => {
      for (const element of document.querySelectorAll('*')) if (element.scrollTop) element.scrollTop = 0;
      window.scrollTo(0, document.scrollingElement.scrollHeight);
    });
    // Only the test list and the opened test: the summary above is the other picture.
    const filter = await page.getByPlaceholder(/^Filter/).boundingBox();
    const top = Math.max(0, Math.round(filter.y) - 22);
    const clip = { x: 0, y: top, width: REPORT_FAILURE.width, height: REPORT_FAILURE.height - top };
    await bothThemes(page, save, 'report', 'failure', undefined, clip);
  } finally {
    await page.context().close();
  }
}

/** Starts `vc ui` and resolves to its one-time sign-in link and a stop function. */
function startUi(vcBin, project) {
  const child = spawn(vcBin, ['ui', '--no-open', '--port', '0'], { cwd: project, env: vcEnv(), stdio: ['ignore', 'pipe', 'inherit'] });
  const exited = new Promise((resolve) => child.on('exit', resolve));
  const stop = async () => {
    child.kill('SIGTERM');
    const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
    await exited;
    clearTimeout(timer);
  };
  return new Promise((resolve, reject) => {
    let output = '';
    child.stdout.on('data', (chunk) => {
      output += chunk;
      const link = /vc ui: (http:\/\/\S+\?token=[A-Za-z0-9_-]+)/.exec(output);
      if (link) resolve({ link: link[1], stop });
    });
    child.on('exit', () => reject(new Error(`vc ui exited before it served:\n${output}`)));
  });
}

/** The web UI: the test tree, a stored run, one of its failures, and the insights over all runs. */
export async function captureWebUi({ browser, save }, { vcBin, project }) {
  const ui = await startUi(vcBin, project);
  const page = await newPage(browser);
  try {
    await page.goto(ui.link);
    const origin = new URL(ui.link).origin;

    // At 1280 px the test tree runs under the details column, so this page is taken wider.
    await page.setViewportSize(WIDE);
    await page.goto(`${origin}/tests`);
    await page.getByText('getsAdaByName').first().click();
    await page.mouse.move(0, 0);
    // Opening the test scrolls the tree to it; its details are at the top, beside the filters.
    await page.evaluate(() => {
      for (const element of document.querySelectorAll('*')) if (element.scrollTop) element.scrollTop = 0;
    });
    await bothThemes(page, save, 'web', 'tests');
    await page.setViewportSize(VIEWPORT);

    await page.goto(`${origin}/runs`);
    await page.getByText('#3 · local').click();
    await page.getByText('Run #3').waitFor();
    await bothThemes(page, save, 'web', 'run');

    await page.getByText('users.profile#getsAdaByName').first().click();
    await page.getByText('Ada Lovelace').first().waitFor();
    await bothThemes(page, save, 'web', 'failure');

    await page.goto(`${origin}/insights`);
    await page.getByText(/flak/i).first().waitFor();
    await bothThemes(page, save, 'web', 'insights');
  } finally {
    await page.context().close();
    await ui.stop();
  }
}
