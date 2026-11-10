use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use rusqlite::{Connection, OptionalExtension as _, Row as SqlRow, Transaction, params};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::time::Instant;
use vc_interfaces::{
    AgentAction, AgentActionWriter, Approval, ApprovalError, ApprovalId, ApprovalRequest, ApprovalStatus, Approvals,
    Decision, StandingRule, Verdict,
};
use vc_model::Method;

use crate::codec::{from_json, json, now_ms, unreadable};
use crate::schema;

/// How often a wait looks at the queue when no file event came: the OS may merge or drop them.
const RECHECK: Duration = Duration::from_secs(1);

/// How each status is spelled in the file.
const STATUSES: [(ApprovalStatus, &str); 4] = [
    (ApprovalStatus::Pending, "pending"),
    (ApprovalStatus::Approved, "approved"),
    (ApprovalStatus::Denied, "denied"),
    (ApprovalStatus::Expired, "expired"),
];

/// The columns a request is read from, in [`approval`]'s order.
const COLUMNS: &str = "id, created_at, agent, tool, profile, methods, summary, arguments, status, decided_by";

/// The agent approval queue, its standing rules and the agent call log, in `history.db`
/// (D195).
///
/// Each call opens its own connection, so every process on the project shares one queue: an
/// agent waiting in `vc mcp` sees a decision made by `vc approve` or the web UI. A wait wakes
/// on file events in the database's folder, and looks again every second in case one was
/// missed.
#[derive(Clone, Debug)]
pub struct SqliteApprovals {
    path: PathBuf,
}

impl SqliteApprovals {
    /// The queue in the history at `path`; nothing is opened until a call.
    #[must_use]
    pub fn new(path: &Path) -> Self {
        Self { path: path.to_path_buf() }
    }

    fn failed(&self, error: &dyn Display) -> ApprovalError {
        ApprovalError::Store { location: self.path.display().to_string(), reason: error.to_string() }
    }

    /// Runs `work` on a connection that may write, creating and migrating the file as needed.
    fn write<T>(&self, work: impl FnOnce(&mut Connection) -> rusqlite::Result<T>) -> Result<T, ApprovalError> {
        let mut connection = schema::open_for_writing(&self.path).map_err(|error| self.failed(&error))?;
        work(&mut connection).map_err(|error| self.failed(&error))
    }

    /// Runs `work` on a read-only connection, or answers the default when there is no file.
    fn read<T: Default>(&self, work: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T, ApprovalError> {
        let Some(connection) = schema::open_for_reading(&self.path).map_err(|error| self.failed(&error))? else {
            return Ok(T::default());
        };
        work(&connection).map_err(|error| self.failed(&error))
    }

    fn status(&self, id: ApprovalId) -> Result<ApprovalStatus, ApprovalError> {
        let text: Option<String> = self.read(|connection| {
            connection.query_row("SELECT status FROM approvals WHERE id = ?1", [id.get()], |row| row.get(0)).optional()
        })?;
        let text = text.ok_or(ApprovalError::Unknown { id })?;
        status(&text).ok_or_else(|| self.failed(&unreadable(0, &text)))
    }

    /// Marks `id` expired when it is still pending, and says where it stands: a decision may
    /// have landed just before.
    fn expire(&self, id: ApprovalId) -> Result<ApprovalStatus, ApprovalError> {
        self.write(|connection| {
            let sql = "UPDATE approvals SET status = 'expired', decided_at = ?1 WHERE id = ?2 AND status = 'pending'";
            connection.execute(sql, params![now_ms(), id.get()]).map(drop)
        })?;
        self.status(id)
    }

    /// Sends on `wake` whenever something in the database's folder changes; `None` when the
    /// OS cannot watch it, and the wait then only looks every [`RECHECK`].
    fn watch(&self, wake: UnboundedSender<()>) -> Option<RecommendedWatcher> {
        let folder = self.path.parent()?;
        let mut watcher = notify::recommended_watcher(move |_: notify::Result<notify::Event>| {
            wake.send(()).ok();
        })
        .ok()?;
        watcher.watch(folder, RecursiveMode::NonRecursive).ok()?;
        Some(watcher)
    }
}

#[async_trait]
impl Approvals for SqliteApprovals {
    fn list(&self, pending_only: bool) -> Result<Vec<Approval>, ApprovalError> {
        self.read(|connection| {
            let only = if pending_only { "WHERE status = 'pending'" } else { "" };
            let sql = format!("SELECT {COLUMNS} FROM approvals {only} ORDER BY id DESC");
            let mut statement = connection.prepare(&sql)?;
            statement.query_map([], approval)?.collect()
        })
    }

    fn get(&self, id: ApprovalId) -> Result<Option<Approval>, ApprovalError> {
        self.read(|connection| {
            let sql = format!("SELECT {COLUMNS} FROM approvals WHERE id = ?1");
            connection.query_row(&sql, [id.get()], approval).optional()
        })
    }

    fn create(&self, request: &ApprovalRequest) -> Result<ApprovalId, ApprovalError> {
        self.write(|connection| {
            connection.execute(
                "INSERT INTO approvals (created_at, agent, tool, profile, methods, summary, arguments, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending')",
                params![
                    now_ms(),
                    &request.agent,
                    &request.tool,
                    &request.profile,
                    json(&method_names(&request.methods))?,
                    &request.summary,
                    &request.arguments,
                ],
            )?;
            Ok(ApprovalId::new(u64::try_from(connection.last_insert_rowid()).unwrap_or_default()))
        })
    }

    fn decide(&self, decision: &Decision) -> Result<ApprovalStatus, ApprovalError> {
        let mut connection = schema::open_for_writing(&self.path).map_err(|error| self.failed(&error))?;
        let transaction = connection.transaction().map_err(|error| self.failed(&error))?;
        let found = standing(&transaction, decision.id).map_err(|error| self.failed(&error))?;
        let Some((status, rule)) = found else { return Err(ApprovalError::Unknown { id: decision.id }) };
        if status != "pending" {
            return Err(ApprovalError::Decided { id: decision.id });
        }
        let decided = settle(&transaction, decision, &rule).and_then(|status| transaction.commit().map(|()| status));
        decided.map_err(|error| self.failed(&error))
    }

    async fn wait(&self, id: ApprovalId, timeout: Duration) -> Result<ApprovalStatus, ApprovalError> {
        let (wake, mut woken) = unbounded_channel();
        let _watching = self.watch(wake.clone());
        let deadline = Instant::now() + timeout;
        loop {
            let status = self.status(id)?;
            let now = Instant::now();
            if status != ApprovalStatus::Pending {
                return Ok(status);
            }
            if now >= deadline {
                return self.expire(id);
            }
            tokio::time::timeout(RECHECK.min(deadline - now), woken.recv()).await.ok();
        }
    }

    fn always_allowed(&self, rule: &StandingRule) -> Result<bool, ApprovalError> {
        self.read(|connection| {
            let sql = "SELECT 1 FROM approval_rules WHERE profile = ?1 AND tool = ?2 AND method = ?3";
            let found: Option<u8> = connection
                .query_row(sql, params![&rule.profile, &rule.tool, rule.method.name()], |row| row.get(0))
                .optional()?;
            Ok(found.is_some())
        })
    }
}

impl AgentActionWriter for SqliteApprovals {
    fn record(&self, action: &AgentAction) -> Result<(), ApprovalError> {
        self.write(|connection| {
            connection
                .execute(
                    "INSERT INTO agent_actions (at, agent, tool, profile, arguments, result, approved_by)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        now_ms(),
                        &action.agent,
                        &action.tool,
                        &action.profile,
                        &action.arguments,
                        &action.result,
                        &action.approved_by,
                    ],
                )
                .map(drop)
        })
    }
}

/// The profile, tool and methods an always-allow of a request covers.
struct Covered {
    profile: String,
    tool: String,
    methods: Vec<String>,
}

/// The status of request `id` and what an always-allow would cover, when there is one.
fn standing(transaction: &Transaction<'_>, id: ApprovalId) -> rusqlite::Result<Option<(String, Covered)>> {
    let sql = "SELECT status, profile, tool, methods FROM approvals WHERE id = ?1";
    transaction
        .query_row(sql, [id.get()], |row| {
            let methods: String = row.get(3)?;
            let covered = Covered { profile: row.get(1)?, tool: row.get(2)?, methods: from_json(3, &methods)? };
            Ok((row.get(0)?, covered))
        })
        .optional()
}

/// Records `decision`, and for an always-allow a standing rule per method.
fn settle(transaction: &Transaction<'_>, decision: &Decision, covered: &Covered) -> rusqlite::Result<ApprovalStatus> {
    let status = match decision.verdict {
        Verdict::Approve | Verdict::AlwaysAllow => ApprovalStatus::Approved,
        Verdict::Deny => ApprovalStatus::Denied,
    };
    let sql = "UPDATE approvals SET status = ?1, decided_at = ?2, decided_by = ?3 WHERE id = ?4";
    transaction.execute(sql, params![status_text(status), now_ms(), &decision.by, decision.id.get()])?;
    if decision.verdict == Verdict::AlwaysAllow {
        let mut add = transaction.prepare(
            "INSERT OR IGNORE INTO approval_rules (profile, tool, method, created_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for method in &covered.methods {
            add.execute(params![&covered.profile, &covered.tool, method, now_ms(), &decision.by])?;
        }
    }
    Ok(status)
}

/// One request, read from [`COLUMNS`].
fn approval(row: &SqlRow<'_>) -> rusqlite::Result<Approval> {
    let methods: String = row.get(5)?;
    let methods: Vec<String> = from_json(5, &methods)?;
    let methods =
        methods.iter().map(|name| Method::named(name).ok_or_else(|| unreadable(5, name))).collect::<Result<_, _>>()?;
    let text: String = row.get(8)?;
    let request = ApprovalRequest {
        agent: row.get(2)?,
        tool: row.get(3)?,
        profile: row.get(4)?,
        methods,
        summary: row.get(6)?,
        arguments: row.get(7)?,
    };
    let status = status(&text).ok_or_else(|| unreadable(8, &text))?;
    Ok(Approval { id: ApprovalId::new(row.get(0)?), request, created_ms: row.get(1)?, status, decided_by: row.get(9)? })
}

fn status(text: &str) -> Option<ApprovalStatus> {
    STATUSES.iter().find(|(_, known)| *known == text).map(|(status, _)| *status)
}

fn status_text(status: ApprovalStatus) -> &'static str {
    STATUSES.iter().find(|(known, _)| *known == status).map_or("pending", |(_, text)| text)
}

fn method_names(methods: &[Method]) -> Vec<&'static str> {
    methods.iter().map(|&method| method.name()).collect()
}

#[cfg(test)]
#[path = "approvals_tests.rs"]
mod tests;
