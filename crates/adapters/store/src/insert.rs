use rusqlite::{Connection, Transaction};
use vc_interfaces::{CheckRow, ExchangeRow, Row, RunEnd, RunId, RunStart, TestRow};
use vc_model::Keep;

use crate::codec::{compress, json, run_status_text, test_status_text};

/// The run and test result an exchange belongs to.
#[derive(Clone, Copy, Debug)]
struct Owner {
    run: RunId,
    test: Option<i64>,
}

/// Adds a run and gives its ID.
pub fn begin(connection: &Connection, start: &RunStart) -> rusqlite::Result<RunId> {
    connection.execute(
        "INSERT INTO runs (started_at, profiles, selector, git_commit, git_branch, seed, vc_version, config_hash, kind)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        (
            start.started_ms,
            json(&start.profiles)?,
            &start.selector,
            &start.git_commit,
            &start.git_branch,
            start.seed.map(u64::cast_signed),
            &start.vc_version,
            &start.config_hash,
            &start.kind,
        ),
    )?;
    Ok(RunId::new(connection.last_insert_rowid().unsigned_abs()))
}

/// Adds `rows` to `run`.
pub fn rows(transaction: &Transaction<'_>, run: RunId, rows: &[Row]) -> rusqlite::Result<()> {
    for row in rows {
        match row {
            Row::Test(test) => self::test(transaction, run, test)?,
            Row::Exchange(exchange) => self::exchange(transaction, Owner { run, test: None }, exchange)?,
        }
    }
    Ok(())
}

/// Closes `run` with `end`, keeping its snapshot.
pub fn finish(transaction: &Transaction<'_>, run: RunId, end: &RunEnd) -> rusqlite::Result<()> {
    let totals = &end.totals;
    transaction.execute(
        "UPDATE runs SET finished_at = ?2, status = ?3, passed = ?4, failed = ?5, blocked = ?6, skipped = ?7, flaky = ?8
         WHERE id = ?1",
        (
            run.get(),
            end.finished_ms,
            run_status_text(end.status),
            totals.passed,
            totals.failed,
            totals.blocked,
            totals.skipped,
            totals.flaky,
        ),
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO snapshots (run_id, data) VALUES (?1, ?2)",
        (run.get(), compress(&end.snapshot)?),
    )?;
    Ok(())
}

/// Deletes the oldest runs beyond `keep`, with everything that belongs to them.
pub fn prune(connection: &Connection, keep: Keep) -> rusqlite::Result<usize> {
    let Keep::Runs(kept) = keep else { return Ok(0) };
    connection.execute("DELETE FROM runs WHERE id NOT IN (SELECT id FROM runs ORDER BY id DESC LIMIT ?1)", [kept])
}

fn test(transaction: &Transaction<'_>, run: RunId, test: &TestRow) -> rusqlite::Result<()> {
    transaction
        .prepare_cached(
            "INSERT INTO test_results (run_id, test_id, example_row, repeat_index, status, duration_ms, attempts,
                                   blocked_by, skip_reason, message)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?
        .execute((
            run.get(),
            &test.test_id,
            test.example_row,
            test.repeat_index,
            test.status.map_or("failed", test_status_text),
            test.duration_ms,
            test.attempts,
            &test.blocked_by,
            &test.skip_reason,
            &test.message,
        ))?;
    let id = transaction.last_insert_rowid();
    test.checks.iter().try_for_each(|check| self::check(transaction, id, check))?;
    test.exchanges.iter().try_for_each(|exchange| self::exchange(transaction, Owner { run, test: Some(id) }, exchange))
}

fn check(transaction: &Transaction<'_>, test: i64, check: &CheckRow) -> rusqlite::Result<()> {
    transaction
        .prepare_cached(
            "INSERT INTO checks (test_result_id, kind, path, expected, actual, passed, origin, file, line)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?
        .execute((
            test,
            &check.kind,
            &check.path,
            &check.expected,
            &check.actual,
            check.passed,
            &check.origin,
            &check.file,
            check.line,
        ))
        .map(drop)
}

fn exchange(transaction: &Transaction<'_>, owner: Owner, exchange: &ExchangeRow) -> rusqlite::Result<()> {
    let timing = &exchange.timing;
    let request_body = exchange.request_body.as_deref().map(compress).transpose()?;
    let response_body = exchange.response_body.as_deref().map(compress).transpose()?;
    transaction
        .prepare_cached(
            "INSERT INTO exchanges (test_result_id, run_id, action, api_id, method, url, status, dns_ms, connect_ms,
                tls_ms, first_byte_ms, download_ms, total_ms, request_headers, response_headers, request_body,
                response_body, body_truncated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        )?
        .execute(rusqlite::params![
            owner.test,
            owner.run.get(),
            &exchange.action,
            &exchange.api_id,
            &exchange.method,
            &exchange.url,
            exchange.status,
            timing.dns_ms,
            timing.connect_ms,
            timing.tls_ms,
            timing.first_byte_ms,
            timing.download_ms,
            timing.total_ms,
            json(&exchange.request_headers)?,
            json(&exchange.response_headers)?,
            request_body,
            response_body,
            exchange.body_truncated,
        ])
        .map(drop)
}
