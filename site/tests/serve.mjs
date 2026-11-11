// Serves the built site (dist/) under the Pages base path for the e2e suite, as GitHub Pages
// does: `astro preview` hands its server to a background process, which Playwright reads as an
// early exit. Usage: node tests/serve.mjs <port>
import { createReadStream, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../dist', import.meta.url));
const base = '/vc-rs/';
const port = Number(process.argv[2] ?? 4329);
const types = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.svg': 'image/svg+xml',
  '.wasm': 'application/wasm',
  '.woff2': 'font/woff2',
  '.txt': 'text/plain; charset=utf-8',
};

function file(url) {
  const path = decodeURIComponent(new URL(url, 'http://x').pathname);
  if (!path.startsWith(base)) return undefined;
  const full = normalize(join(root, path.slice(base.length)));
  if (full !== root && !full.startsWith(root + sep)) return undefined;
  try {
    return statSync(full).isDirectory() ? join(full, 'index.html') : full;
  } catch {
    return undefined;
  }
}

createServer((request, response) => {
  const found = file(request.url ?? '/');
  if (!found) return response.writeHead(404).end('not found');
  response.writeHead(200, { 'content-type': types[extname(found)] ?? 'application/octet-stream' });
  createReadStream(found)
    .on('error', () => response.destroy())
    .pipe(response);
}).listen(port, '127.0.0.1');
