// Makes the scratch project's history the same on every take. Each run stores its times, its
// durations and the order its files happened to start in; the screens show all three. Here they are
// rewritten to fixed values: runs start `RUN_GAP_MS` apart from `FIRST_RUN_AT`, each duration becomes
// a stand-in near the measured one, and rows are put in test ID order.
import { join } from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import { zstdCompressSync, zstdDecompressSync } from 'node:zlib';
import { FIRST_RUN_AT } from './project.mjs';

const RUN_GAP_MS = 47 * 60_000;
/** The demo's process id, which it sends in a header. */
const DEMO_PID = '4242';
/** The seed every run shows: the TUI's runs pick a random one. */
const SEED = 42;
/** How many iterations the load run shows passing: the count a 20 s run gets differs a little each time. */
const LOAD_ITERATIONS = 930;

/** A number in [0, 1) that depends only on `key` (FNV-1a). */
export function hash(key) {
  let h = 0x811c9dc5;
  for (const char of key) h = Math.imul(h ^ char.codePointAt(0), 0x01000193) >>> 0;
  return h / 2 ** 32;
}

/** A stable whole-millisecond duration near `measured`: a few ms for a quick call, the hundreds kept otherwise. */
function fixedMs(measured, key) {
  if (!measured) return 0;
  if (measured < 50) return 1 + Math.floor(hash(key) * 6);
  return Math.floor(measured / 100) * 100 + 18 + Math.floor(hash(key) * 6);
}

const testKey = (testId, row, repeat) => `${testId}:${row ?? '-'}:${repeat ?? 0}`;

function fixHeaders(headers, startedAt) {
  return headers.map(([name, value]) => {
    if (name === 'date') return [name, new Date(startedAt).toUTCString()];
    if (name === 'x-vc-demo-pid') return [name, DEMO_PID];
    return [name, value];
  });
}

/** An exchange's timings in milliseconds as fixed shares of `ms`; a phase not measured stays null. */
function timings(ms, measured) {
  const share = (key, part) => (measured[key] === null || measured[key] === undefined ? null : Math.round(ms * part * 100) / 100);
  return { dns: share('dns', 0.1), connect: share('connect', 0.15), tls: share('tls', 0.2), firstByte: share('firstByte', 0.85), download: share('download', 0.1), total: ms };
}

/**
 * Gives the rows new ids in a fixed order: the screens list a run's tests and requests by id, and
 * `vc run` starts files in an order that changes from run to run. Each row moves to minus its new id
 * first, so no two rows share an id on the way.
 */
function renumber(db) {
  const move = (table, rows, children) => {
    if (!rows.length) return;
    const start = Math.min(...rows.map((row) => row.id));
    rows.forEach((row, i) => {
      db.prepare(`UPDATE ${table} SET id = ? WHERE id = ?`).run(-(start + i), row.id);
      for (const [child, column] of children) db.prepare(`UPDATE ${child} SET ${column} = ? WHERE ${column} = ?`).run(-(start + i), row.id);
    });
    db.exec(`UPDATE ${table} SET id = -id WHERE id < 0`);
    for (const [child, column] of children) db.exec(`UPDATE ${child} SET ${column} = -${column} WHERE ${column} < 0`);
  };
  move('test_results', db.prepare('SELECT id FROM test_results ORDER BY run_id, test_id, example_row, repeat_index, id').all(), [
    ['checks', 'test_result_id'],
    ['exchanges', 'test_result_id'],
  ]);
  const exchanges = db.prepare('SELECT id FROM exchanges ORDER BY run_id, test_result_id IS NULL, test_result_id, id').all();
  move('exchanges', exchanges, []);
}

/** Fixes one run's tests and exchanges in the tables; returns the sum of its tests' durations. */
function fixRows(db, run, startedAt) {
  let total = 0;
  const tests = db.prepare('SELECT id, test_id, example_row, repeat_index, duration_ms FROM test_results WHERE run_id = ?').all(run.id);
  for (const test of tests) {
    const key = testKey(test.test_id, test.example_row, test.repeat_index);
    const ms = fixedMs(test.duration_ms, key);
    total += ms;
    db.prepare('UPDATE test_results SET duration_ms = ? WHERE id = ?').run(ms, test.id);
    const exchanges = db.prepare('SELECT * FROM exchanges WHERE test_result_id = ? ORDER BY id').all(test.id);
    exchanges.forEach((x, n) => fixExchangeRow(db, x, fixedMs(x.total_ms, `${key} ${x.method} ${x.url} ${n}`), startedAt));
  }
  for (const x of db.prepare('SELECT * FROM exchanges WHERE run_id = ? AND test_result_id IS NULL').all(run.id)) {
    fixExchangeRow(db, x, fixedMs(x.total_ms, `${x.method} ${x.url}`), startedAt);
  }
  return total;
}

function fixExchangeRow(db, x, ms, startedAt) {
  const t = timings(ms, { dns: x.dns_ms, connect: x.connect_ms, tls: x.tls_ms, firstByte: x.first_byte_ms, download: x.download_ms });
  const headers = (json) => (json ? JSON.stringify(fixHeaders(JSON.parse(json), startedAt)) : json);
  db.prepare(
    `UPDATE exchanges SET total_ms = ?, dns_ms = ?, connect_ms = ?, tls_ms = ?, first_byte_ms = ?, download_ms = ?,
       request_headers = ?, response_headers = ? WHERE id = ?`,
  ).run(t.total, t.dns, t.connect, t.tls, t.firstByte, t.download, headers(x.request_headers), headers(x.response_headers), x.id);
}

/** Fixes the report snapshot `vc report` writes a run's reports from, the same way as its rows. */
function fixSnapshot(db, runId, startedAt, length) {
  const row = db.prepare('SELECT data FROM snapshots WHERE run_id = ?').get(runId);
  if (!row) return;
  const snapshot = JSON.parse(zstdDecompressSync(row.data).toString('utf8'));
  Object.assign(snapshot.run, { startedMs: startedAt, durationMs: length, seed: snapshot.run.seed === null ? null : SEED });
  snapshot.files.sort();
  snapshot.tests.sort((a, b) => testKey(a.id, a.exampleRow, a.repeatIndex).localeCompare(testKey(b.id, b.exampleRow, b.repeatIndex)));
  let clock = 0;
  for (const test of snapshot.tests) {
    const key = testKey(test.id, test.exampleRow, test.repeatIndex);
    if (test.startMs !== null) {
      const ms = fixedMs(Math.max(test.stopMs - test.startMs, 1), key);
      Object.assign(test, { startMs: clock, stopMs: clock + ms });
      clock += ms;
    }
    test.exchanges.forEach((x, n) => {
      const ms = fixedMs(Math.max(x.timing.total, 1), `${key} ${x.method} ${x.url} ${n}`);
      Object.assign(x, { timing: timings(ms, x.timing), requestHeaders: fixHeaders(x.requestHeaders, startedAt), responseHeaders: fixHeaders(x.responseHeaders, startedAt) });
    });
  }
  const data = zstdCompressSync(Buffer.from(JSON.stringify(snapshot)));
  db.prepare('UPDATE snapshots SET data = ? WHERE run_id = ?').run(data, runId);
}

/** The project's stored runs, oldest first: each one's number and kind (`run` or `load`). */
export function storedRuns(project) {
  const db = new DatabaseSync(join(project, '.vibranium/history.db'), { readOnly: true });
  try {
    return db.prepare('SELECT id, kind FROM runs ORDER BY id').all().map(({ id, kind }) => ({ id, kind }));
  } finally {
    db.close();
  }
}

/** Rewrites the project's stored runs so the same steps give the same history. */
export function fixHistory(project) {
  const db = new DatabaseSync(join(project, '.vibranium/history.db'));
  const runs = db.prepare('SELECT id, started_at, finished_at, kind FROM runs ORDER BY id').all();
  db.exec('PRAGMA foreign_keys = OFF');
  db.exec('BEGIN');
  renumber(db);
  runs.forEach((run, i) => {
    const startedAt = FIRST_RUN_AT + i * RUN_GAP_MS;
    const total = fixRows(db, run, startedAt);
    // A load run keeps its length in whole seconds; a test run lasts as long as its tests and a little.
    const length = run.kind === 'run' ? total + 38 : Math.round((run.finished_at - run.started_at) / 1000) * 1000;
    db.prepare('UPDATE runs SET started_at = ?, finished_at = ?, seed = CASE WHEN seed IS NULL THEN NULL ELSE ? END WHERE id = ?').run(startedAt, startedAt + length, SEED, run.id);
    if (run.kind === 'run') fixSnapshot(db, run.id, startedAt, length);
    else db.prepare('UPDATE runs SET passed = ? WHERE id = ?').run(LOAD_ITERATIONS, run.id);
  });
  db.exec('COMMIT');
  db.exec('PRAGMA wal_checkpoint(TRUNCATE)');
  db.close();
}
