import '#lib/styles/tokens.css';
import '#lib/styles/controls.css';
import './report.css';
import { mount } from 'svelte';
import Report from './Report.svelte';
import { Report as ReportState } from './report.svelte.ts';
import { DataStore, inflate } from './store.ts';

// The HTML report's script (D169). The page runs it inline, then the data files call `VIB.load`
// and the page calls `VIB.start()`. It needs no server and makes no network request.

/** Folder layout: each key is the script `data/<key>.js`, added on first use. */
const store = new DataStore((key, failed) => {
  const script = document.createElement('script');
  script.src = `data/${key}.js`;
  script.onerror = failed;
  document.body.append(script);
});

/** A data file's call. */
export function load(key: string, value: unknown): void {
  store.load(key, value);
}

/** Inflates the single-file layout's data if it has some, then shows the report. */
export function start(): void {
  const bundled = document.getElementById('vib-data')?.textContent;
  if (bundled) store.useBundle(inflate(bundled));
  const report = new ReportState(store, (hash) => {
    if (location.hash !== hash) location.hash = hash;
  });
  mount(Report, { target: document.getElementById('app') ?? document.body, props: { report } });
  window.addEventListener('hashchange', () => void report.route(location.hash));
  void report.start().then(() => report.route(location.hash));
}
