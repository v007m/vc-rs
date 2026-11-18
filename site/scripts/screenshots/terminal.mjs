// Runs a command in a pseudo-terminal, emulates the screen with @xterm/headless and renders the
// screen as HTML, so Playwright can take its picture. No real terminal window is involved.
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import xterm from '@xterm/headless';

const { Terminal } = xterm;
const require = createRequire(import.meta.url);

/** Quotes one word for `sh -c`. */
const quote = (word) => `'${String(word).replaceAll("'", `'\\''`)}'`;

/** Printed after the command ends: an xterm title change, which draws nothing. */
const DONE = '\u001b]0;vc-screenshots-done\u0007';

/** The `script` call that gives `line` a pseudo-terminal of the given size. */
function ptyCommand(line, { cols, rows }) {
  const inner = `stty cols ${cols} rows ${rows}; ${line}; printf '\\033]0;vc-screenshots-done\\007'`;
  // Node's pipes are sockets, which macOS `script` cannot configure; `cat` turns them into pipes.
  if (process.platform === 'darwin') return `cat | script -q /dev/null sh -c ${quote(inner)} | cat`;
  return `script -qfec ${quote(inner)} /dev/null`;
}

/**
 * A command running in an emulated terminal of `cols` by `rows` cells. With `rewrite`, the output is
 * held back until the command ends and then shown after `rewrite` changed its text, so text whose
 * width varies (a path, a duration) can be made fixed before the terminal lays it out.
 */
export function openTerminal({ argv, cwd, env = process.env, cols = 120, rows = 34, rewrite }) {
  const line = argv.map(quote).join(' ');
  const child = spawn('sh', ['-c', ptyCommand(line, { cols, rows })], {
    cwd,
    env: { ...env, TERM: 'xterm-256color', COLORTERM: 'truecolor' },
    stdio: ['pipe', 'pipe', 'inherit'],
    // Its own process group, so `close` ends `script`, the shell and the program together.
    detached: true,
  });
  const term = new Terminal({ cols, rows, allowProposedApi: true, scrollback: 0 });
  let lastOutput = Date.now();
  // The whole output, kept only when it is rewritten; otherwise just enough to find DONE split across chunks.
  let raw = '';
  let tail = '';
  let done = false;
  let finish;
  const exited = new Promise((resolve) => (finish = resolve));
  // The terminal answers the program's queries (cursor position, colours) on its stdin.
  term.onData((data) => child.stdin.writable && child.stdin.write(data));
  child.stdout.setEncoding('utf8');
  child.stdout.on('data', (chunk) => {
    lastOutput = Date.now();
    if (done) return;
    if (rewrite) raw += chunk;
    else term.write(chunk);
    const recent = tail + chunk;
    tail = recent.slice(-DONE.length);
    if (!recent.includes(DONE)) return;
    done = true;
    if (rewrite) term.write(rewrite(raw.slice(0, raw.indexOf(DONE))), finish);
    else term.write('', finish);
  });
  const flush = () => new Promise((resolve) => term.write('', resolve));
  return {
    term,
    /** Settles once the command has ended and its output is on the screen. */
    exited,
    text: () => screenText(term),
    send: (keys) => child.stdin.write(keys),
    /** Waits until the screen shows `pattern`, then until the output has been quiet for `quietMs`. */
    async waitFor(pattern, { timeoutMs = 30_000, quietMs = 400 } = {}) {
      const deadline = Date.now() + timeoutMs;
      const matches = () => (typeof pattern === 'string' ? screenText(term).includes(pattern) : pattern.test(screenText(term)));
      for (;;) {
        await flush();
        if (matches() && Date.now() - lastOutput >= quietMs) return;
        if (Date.now() > deadline) throw new Error(`screen never showed ${pattern}:\n${screenText(term)}`);
        await sleep(50);
      }
    },
    async close() {
      await Promise.race([exited, sleep(3000)]);
      child.stdin.end();
      try {
        process.kill(-child.pid, 'SIGTERM');
      } catch {
        // The group has already ended.
      }
      child.stdout.destroy();
      term.dispose();
    },
  };
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** The visible screen as plain text, one line per row. */
export function screenText(term) {
  const buffer = term.buffer.active;
  const lines = [];
  for (let y = 0; y < term.rows; y++) lines.push(buffer.getLine(buffer.viewportY + y)?.translateToString(true) ?? '');
  return lines.join('\n');
}

function colour(cell, kind) {
  if (kind === 'fg') {
    if (cell.isFgRGB()) return { rgb: cell.getFgColor() };
    if (cell.isFgPalette()) return { index: cell.getFgColor() };
  } else {
    if (cell.isBgRGB()) return { rgb: cell.getBgColor() };
    if (cell.isBgPalette()) return { index: cell.getBgColor() };
  }
  return null;
}

/** The visible screen as rows of styled cells; wide characters take one cell. */
export function snapshot(term) {
  const buffer = term.buffer.active;
  const rows = [];
  for (let y = 0; y < term.rows; y++) {
    const line = buffer.getLine(buffer.viewportY + y);
    const cells = [];
    for (let x = 0; line && x < term.cols; x++) {
      const cell = line.getCell(x);
      if (!cell || cell.getWidth() === 0) continue;
      cells.push({
        ch: cell.getChars() || ' ',
        width: cell.getWidth(),
        fg: colour(cell, 'fg'),
        bg: colour(cell, 'bg'),
        bold: !!cell.isBold(),
        dim: !!cell.isDim(),
        italic: !!cell.isItalic(),
        underline: !!cell.isUnderline(),
        inverse: !!cell.isInverse(),
      });
    }
    rows.push(cells);
  }
  return rows;
}

/** Drops the blank rows at the bottom of a screen, keeping at least one. */
export function trimBottom(rows) {
  let end = rows.length;
  while (end > 1 && rows[end - 1].every((cell) => cell.ch.trim() === '' && !cell.bg)) end--;
  return rows.slice(0, end);
}

/** The colour of 256-colour palette entry `index` beyond the 16 the theme sets. */
function xterm256(index) {
  if (index >= 232) {
    const level = 8 + (index - 232) * 10;
    return (level << 16) | (level << 8) | level;
  }
  const cube = index - 16;
  const step = (n) => (n === 0 ? 0 : 55 + n * 40);
  return (step(Math.floor(cube / 36)) << 16) | (step(Math.floor(cube / 6) % 6) << 8) | step(cube % 6);
}

function paint(colourSpec, theme, fallback) {
  if (!colourSpec) return fallback;
  const value = colourSpec.rgb ?? (colourSpec.index < 16 ? null : xterm256(colourSpec.index));
  if (value === null) return theme.palette[colourSpec.index];
  return `#${value.toString(16).padStart(6, '0')}`;
}

function style(cell, theme) {
  let fg = paint(cell.fg, theme, theme.fg);
  let bg = paint(cell.bg, theme, null);
  if (cell.inverse) [fg, bg] = [bg ?? theme.bg, fg];
  const rules = [`color:${fg}`];
  if (bg) rules.push(`background:${bg}`);
  if (cell.bold) rules.push('font-weight:700');
  if (cell.dim) rules.push('opacity:.6');
  if (cell.italic) rules.push('font-style:italic');
  if (cell.underline) rules.push('text-decoration:underline');
  return rules.join(';');
}

const escape = (text) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');

function rowHtml(cells, theme) {
  const spans = [];
  for (const cell of cells) {
    const css = style(cell, theme);
    const last = spans.at(-1);
    // A wide character gets a box two cells wide, so the columns after it stay aligned.
    if (cell.width === 1 && last && !last.wide && last.css === css) last.text += cell.ch;
    else spans.push({ css, text: cell.ch, wide: cell.width === 2 });
  }
  const html = spans
    .map(({ css, text, wide }) => `<span${wide ? ' class="w"' : ''} style="${css}">${escape(text)}</span>`)
    .join('');
  return `<div class="row">${html || ' '}</div>`;
}

function fontFace(weight) {
  const file = require.resolve(`@fontsource/jetbrains-mono/files/jetbrains-mono-latin-${weight}-normal.woff2`);
  const data = readFileSync(file).toString('base64');
  return `@font-face{font-family:'JetBrains Mono';font-weight:${weight};src:url(data:font/woff2;base64,${data}) format('woff2')}`;
}

/** A standalone HTML page that draws `rows` with `theme`'s colours. */
export function renderHtml(rows, theme, { cols }) {
  const body = rows.map((cells) => rowHtml(cells, theme)).join('');
  return `<!doctype html><html><head><meta charset="utf-8"><style>
${fontFace(400)}${fontFace(700)}
html,body{margin:0;background:transparent}
.screen{display:inline-block;padding:14px 18px;background:${theme.bg};color:${theme.fg};
  font:13px/17px 'JetBrains Mono',ui-monospace,monospace;font-variant-ligatures:none;width:${cols}ch}
.row{white-space:pre;height:1.25em;overflow:hidden}
.w{display:inline-block;width:2ch;text-align:center}
</style></head><body><div class="screen">${body}</div></body></html>`;
}

/** Terminal colours for a light and a dark page. The TUI paints its own colours over them. */
export const TERMINAL_THEMES = {
  dark: {
    bg: '#0f1115',
    fg: '#d7dae0',
    palette: ['#1e2127', '#f07178', '#7fd98a', '#e5c07b', '#7aa2f7', '#c678dd', '#56b6c2', '#c8ccd4',
      '#5c6370', '#ff8b92', '#98e6a2', '#f0d08f', '#99b8ff', '#d79ae8', '#7ccdd8', '#ffffff'],
  },
  light: {
    bg: '#fafafa',
    fg: '#24292f',
    palette: ['#24292f', '#cf222e', '#1a7f37', '#9a6700', '#0969da', '#8250df', '#1b7c83', '#6e7781',
      '#57606a', '#a40e26', '#116329', '#7d4e00', '#0550ae', '#6639ba', '#1b7c83', '#8c959f'],
  },
};
