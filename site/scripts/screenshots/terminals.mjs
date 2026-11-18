// The terminal pictures: `vc run` as it prints, and the TUI's five tabs.
import { realpathSync } from 'node:fs';
import { TERMINAL_THEMES, openTerminal, renderHtml, screenText, sleep, snapshot, trimBottom } from './terminal.mjs';
import { hash } from './history.mjs';
import { vcEnv } from './project.mjs';

const ANSI = /\u001b\[[0-9;]*m/g;

/**
 * A stand-in for a measured duration with as many digits: the first digit is kept and the rest come
 * from `key`, so a 122 ms call stays near 120 ms and the columns do not move.
 */
function steadyDigits(digits, key) {
  const h = hash(key);
  if (digits.length === 1) return String(1 + Math.floor(h * 5));
  const rest = String(Math.floor(h * 10 ** (digits.length - 1))).padStart(digits.length - 1, '0');
  return digits[0] + rest;
}

/**
 * `steadyDigits` for the whole milliseconds of a duration. A quick call takes a single digit, padded
 * on the left to the width measured, so a first call slowed by a new connection (`10ms`) does not
 * shift the line.
 */
function steadyWhole(whole, key) {
  if (Number(whole) >= 50) return steadyDigits(whole, key);
  return ' '.repeat(whole.length - 1) + steadyDigits('0', key);
}

/** Steadies each `<n>ms` on a line, keyed by the line's other text. */
function steadyLine(line) {
  const key = line.replace(ANSI, '').replace(/\d+(?:\.\d+)? ?m?s\b/g, '');
  let n = 0;
  return line.replace(/(\d+)(?:\.(\d+))?( ?m?s)\b/g, (match, whole, fraction, unit, offset) => {
    // A number glued to a word (`p95`) is not a duration.
    if (/[A-Za-z]$/.test(line.slice(0, offset))) return match;
    const steady = steadyWhole(whole, `${key}#${n++}`);
    return fraction === undefined ? `${steady}${unit}` : `${steady}.${steadyDigits(fraction, `${key}#f${n}`)}${unit}`;
  });
}

/**
 * `vc run` schedules files in an order that changes from run to run; the printed groups are put in
 * name order so the picture does not. Each group is a block of lines after a blank line.
 */
function sortGroups(text) {
  const lines = text.split('\r\n');
  const summary = lines.findIndex((line) => /\d+ passed/.test(line.replace(ANSI, '')));
  const head = lines.slice(0, 2);
  const blocks = lines.slice(2, summary - 1).join('\r\n').split('\r\n\r\n');
  blocks.sort((a, b) => a.replace(ANSI, '').localeCompare(b.replace(ANSI, '')));
  return [...head, blocks.join('\r\n\r\n'), ...lines.slice(summary - 1)].join('\r\n');
}

/** Run 1 in a pseudo-terminal, as `vc run` prints it, with the project path shown as `~/shop`. */
export async function consoleRun({ vcBin, project }) {
  const paths = [realpathSync(project), project];
  const rewrite = (raw) => {
    let text = paths.reduce((out, path) => out.replaceAll(path, '~/shop'), raw);
    text = sortGroups(text);
    return text.split('\r\n').map(steadyLine).join('\r\n');
  };
  const terminal = openTerminal({ argv: [vcBin, 'run', '--seed', '42'], cwd: project, env: vcEnv(), cols: 104, rows: 60, rewrite });
  await terminal.exited;
  const text = screenText(terminal.term);
  const rows = trimBottom(snapshot(terminal.term));
  await terminal.close();
  // The terminal keeps no scrollback, so output taller than it would lose its first lines.
  if (!text.startsWith('plan ')) throw new Error(`\`vc run\` no longer fits in its terminal; raise its rows:\n${text}`);
  return rows;
}

/** Moves the cursor down until the screen's selected line (`▸`, or the highlighted row) shows `text`. */
async function select(terminal, text, { step = 'j', limit = 40 } = {}) {
  for (let i = 0; i < limit; i++) {
    const selected = terminal.text().split('\n').find((line) => line.includes('▸'));
    if (selected?.includes(text)) return;
    terminal.send(step);
    await sleep(120);
  }
  throw new Error(`could not select ${text}:\n${terminal.text()}`);
}

const shot = (terminal) => snapshot(terminal.term);

/**
 * Drives the TUI: Run (run 3), Explore (a test explained), Load (the smoke scenario, run 4) and
 * Import (an OpenAPI spec from the demo, drafted). Then `fixHistory` makes the stored times fixed and
 * a second session shows History with a compare, then the help panel over it. Run and Load are not
 * taken: the requests a parallel run lists last and a live load test's figures change every time.
 */
export async function tuiSessions({ vcBin, project, fixHistory, specUrl }) {
  const open = () => openTerminal({ argv: [vcBin, 'tui'], cwd: project, env: vcEnv(), cols: 120, rows: 34 });
  const screens = {};
  let terminal = open();
  try {
    console.log('  tui: run');
    await terminal.waitFor('no run yet');
    terminal.send('a');
    await terminal.waitFor(/(passed|failed) 19\/19/, { quietMs: 800 });

    console.log('  tui: explore');
    terminal.send('2');
    await terminal.waitFor('enter explains');
    await select(terminal, 'getsCreatedUser');
    terminal.send('\r');
    await terminal.waitFor('Explain');
    await terminal.waitFor('users#get');
    screens.explore = shot(terminal);

    console.log('  tui: load');
    terminal.send('4');
    await terminal.waitFor('1 scenario');
    terminal.send('\r');
    await terminal.waitFor(/finished after/, { timeoutMs: 60_000, quietMs: 1000 });

    console.log('  tui: import');
    terminal.send('5');
    await terminal.waitFor('choose the importer');
    terminal.send('j');
    await sleep(150);
    terminal.send('\r');
    await sleep(150);
    terminal.send(`${specUrl}\r`);
    await sleep(150);
    // The demo's spec as a new `people` service: `users` already has an api file.
    terminal.send('j\r');
    await sleep(150);
    terminal.send('people\r');
    await sleep(150);
    terminal.send('j ');
    await sleep(150);
    terminal.send('d');
    await terminal.waitFor(/\+\+\+|@@|new file/, { timeoutMs: 15_000, quietMs: 600 });
    screens.import = shot(terminal);
    terminal.send('q');
  } finally {
    await terminal.close();
  }

  fixHistory();
  terminal = open();
  try {
    console.log('  tui: history');
    terminal.send('3');
    await terminal.waitFor('stored runs');
    await select(terminal, '#3 ');
    terminal.send('\r');
    await terminal.waitFor('Compare #2 → #3', { quietMs: 600 });
    screens.history = shot(terminal);

    console.log('  tui: help');
    terminal.send('?');
    await terminal.waitFor('Help', { quietMs: 400 });
    screens.help = shot(terminal);
    terminal.send('?');
    await sleep(150);
    terminal.send('q');
  } finally {
    await terminal.close();
  }
  return screens;
}

/** Draws each screen as HTML and takes its picture: `vc run` in both themes, the TUI in its own. */
export async function renderTerminals({ browser, save }, { cli, tui }) {
  const page = await browser.newPage({ deviceScaleFactor: 2, viewport: { width: 1200, height: 800 } });
  const picture = async (rows, theme, cols) => {
    await page.setContent(renderHtml(rows, TERMINAL_THEMES[theme], { cols }));
    await page.evaluate(() => document.fonts.ready);
    return page.locator('.screen').screenshot({ omitBackground: true });
  };
  try {
    if (cli) for (const theme of ['light', 'dark']) await save(await picture(cli, theme, 104), 'cli', `run-${theme}`);
    for (const [name, rows] of Object.entries(tui ?? {})) await save(await picture(rows, 'dark', 120), 'tui', `${name}-dark`);
  } finally {
    await page.close();
  }
}
