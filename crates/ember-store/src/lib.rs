//! Durable state for Ember POS.
//!
//! Two tables. `actions` is an append-only log of everything that happened
//! during service; `snapshot` holds the reduced state so start-up does not have
//! to replay the log. The log is the interesting half — it is both the audit
//! trail and the training data the Python services will later learn from, which
//! is why nothing in here ever updates or deletes a row in it.

pub mod auth;

use std::path::Path;
use std::sync::Mutex;

use ember_core::{reduce, Action, PosState, Rejection};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not encode or decode stored state: {0}")]
    Json(#[from] serde_json::Error),
    #[error("store mutex was poisoned")]
    Poisoned,
    #[error("a PIN must be 4 to 12 digits")]
    WeakPin,
    #[error("could not hash or verify a credential")]
    PasswordHash,
    #[error(
        "database is at schema version {found}, but this build only knows {known}; \
         it was written by a newer version of Ember POS"
    )]
    FutureSchema { found: i64, known: i64 },
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// The current state together with the revision that produced it.
///
/// `version` increments once per applied action; clients use it to tell whether
/// a snapshot they hold is stale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub version: i64,
    pub state: PosState,
}

/// One row of the append-only action log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggedAction {
    pub seq: i64,
    pub action: Action,
}

/// Outcome of submitting an action.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    /// The action changed the state; here is the new revision.
    Changed(Revision),
    /// A guard refused the action, and this is why. Distinct from `Unchanged`:
    /// a refusal is something staff need told, a no-op is not.
    Rejected(Rejection),
    /// Allowed, but it would not have changed anything — re-saving identical
    /// notes, for instance. Nothing to persist and nothing to report.
    Unchanged,
    /// This action id was already applied. Re-submitting is a no-op, so that a
    /// retried request cannot seat a party or fire an order twice.
    Duplicate,
}

/// One schema migration. `sql` runs exactly once, in a transaction.
struct Migration {
    /// Named only so a failure says which step broke.
    name: &'static str,
    sql: &'static str,
}

/// Ordered schema migrations, oldest first.
///
/// The number applied is recorded in SQLite's `user_version`, so each one runs
/// once per database. **Never edit or reorder a migration that has shipped** —
/// append a new one instead, because databases in the field have already run
/// the old text and will not run it again.
///
/// Migration 0 keeps `IF NOT EXISTS` deliberately: databases created before
/// this runner existed already have these tables and a `user_version` of 0, so
/// replaying it over them has to be a no-op rather than an error.
const MIGRATIONS: &[Migration] = &[
    Migration {
        name: "initial schema",
        sql: r#"
CREATE TABLE IF NOT EXISTS actions (
    seq     INTEGER PRIMARY KEY AUTOINCREMENT,
    id      TEXT NOT NULL UNIQUE,
    at      TEXT NOT NULL,
    kind    TEXT NOT NULL,
    payload TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS actions_kind_idx ON actions (kind);

CREATE TABLE IF NOT EXISTS snapshot (
    id      INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL,
    state   TEXT NOT NULL
);
"#,
    },
    Migration {
        name: "staff credentials and terminal sessions",
        sql: r#"
CREATE TABLE staff_credentials (
    staff_id     TEXT PRIMARY KEY,
    pin_hash     TEXT NOT NULL,
    failed_count INTEGER NOT NULL DEFAULT 0,
    locked_until TEXT,
    updated_at   TEXT NOT NULL
);

-- Sessions are stored by the SHA-256 of the token, never the token itself, so
-- a copy of this database does not hand anyone a working session.
CREATE TABLE sessions (
    token_hash   TEXT PRIMARY KEY,
    staff_id     TEXT NOT NULL,
    terminal_id  TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    expires_at   TEXT NOT NULL
);

CREATE INDEX sessions_expires_idx ON sessions (expires_at);
"#,
    },
    Migration {
        name: "record who performed each action",
        sql: r#"
-- The log recorded what happened and never who did it, so the audit trail
-- could not answer the one question an audit trail exists for. Existing rows
-- keep NULL: those actions genuinely have no known actor, and inventing one
-- would be worse than admitting it.
ALTER TABLE actions ADD COLUMN actor_staff_id TEXT;
ALTER TABLE actions ADD COLUMN terminal_id TEXT;
-- `at` is the client's claim; this is when the server actually took it.
ALTER TABLE actions ADD COLUMN received_at TEXT;
"#,
    },
];

pub struct Store {
    connection: Mutex<rusqlite::Connection>,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(rusqlite::Connection::open(path)?)
    }

    /// In-memory store, for tests and for `--ephemeral` runs.
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(rusqlite::Connection::open_in_memory()?)
    }

    fn from_connection(mut connection: rusqlite::Connection) -> Result<Self> {
        // WAL keeps a reader (the SSE snapshot fetch) from blocking a writer.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Self::migrate(&mut connection)?;

        let store = Self {
            connection: Mutex::new(connection),
        };
        store.seed_if_empty()?;
        Ok(store)
    }

    /// Bring a database up to the current schema.
    ///
    /// Each pending migration runs in its own transaction and bumps
    /// `user_version` in the same transaction, so an interrupted upgrade leaves
    /// the database at the last version that fully applied rather than
    /// half-migrated.
    fn migrate(connection: &mut rusqlite::Connection) -> Result<()> {
        let applied: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let known = MIGRATIONS.len() as i64;

        // A database written by a newer build may have columns and constraints
        // this binary does not understand. Refusing to open it is the only safe
        // answer: writing to it would silently corrupt the newer shape.
        if applied > known {
            return Err(StoreError::FutureSchema {
                found: applied,
                known,
            });
        }

        for (index, migration) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
            let transaction = connection.transaction()?;
            transaction.execute_batch(migration.sql).map_err(|error| {
                eprintln!(
                    "ember-store: migration {:?} failed: {error}",
                    migration.name
                );
                error
            })?;
            transaction.pragma_update(None, "user_version", index as i64 + 1)?;
            transaction.commit()?;
        }
        Ok(())
    }

    /// The schema version this database is at. Surfaced by the health endpoint
    /// so a deployment can tell whether a migration actually landed.
    pub fn schema_version(&self) -> Result<i64> {
        let connection = self.lock()?;
        Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>> {
        self.connection.lock().map_err(|_| StoreError::Poisoned)
    }

    fn seed_if_empty(&self) -> Result<()> {
        let connection = self.lock()?;
        let existing: i64 =
            connection.query_row("SELECT COUNT(*) FROM snapshot", [], |row| row.get(0))?;
        if existing == 0 {
            connection.execute(
                "INSERT INTO snapshot (id, version, state) VALUES (1, 0, ?1)",
                [serde_json::to_string(&ember_core::seed::initial_state())?],
            )?;
        }
        Ok(())
    }

    /// Current state and version.
    pub fn revision(&self) -> Result<Revision> {
        let connection = self.lock()?;
        Self::read_revision(&connection)
    }

    fn read_revision(connection: &rusqlite::Connection) -> Result<Revision> {
        let (version, state): (i64, String) = connection.query_row(
            "SELECT version, state FROM snapshot WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(Revision {
            version,
            state: serde_json::from_str(&state)?,
        })
    }

    /// Reduces `action` into the current state and, if it changed anything,
    /// commits the new state and the log entry together.
    pub fn apply(&self, action: &Action) -> Result<Applied> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;

        let already: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM actions WHERE id = ?1",
            [&action.id],
            |row| row.get(0),
        )?;
        if already > 0 {
            return Ok(Applied::Duplicate);
        }

        let current = Self::read_revision(&transaction)?;
        let next_state = match reduce(&current.state, action) {
            Ok(Some(next)) => next,
            Ok(None) => return Ok(Applied::Unchanged),
            Err(reason) => return Ok(Applied::Rejected(reason)),
        };
        let version = current.version + 1;

        // actor and received_at are columns as well as being inside the payload:
        // an audit query should not have to parse JSON to answer "what did this
        // person do tonight".
        transaction.execute(
            "INSERT INTO actions (id, at, kind, payload, actor_staff_id, terminal_id, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                &action.id,
                &action.at,
                action.kind.label(),
                serde_json::to_string(action)?,
                action.actor.as_ref().map(|actor| actor.staff_id.as_str()),
                action
                    .actor
                    .as_ref()
                    .map(|actor| actor.terminal_id.as_str()),
                chrono::Utc::now().to_rfc3339(),
            ),
        )?;
        transaction.execute(
            "UPDATE snapshot SET version = ?1, state = ?2 WHERE id = 1",
            (version, serde_json::to_string(&next_state)?),
        )?;
        transaction.commit()?;

        Ok(Applied::Changed(Revision {
            version,
            state: next_state,
        }))
    }

    /// The append-only log, oldest first. This is what the forecasting and
    /// ranking services in `services/brain` will train on.
    pub fn actions(&self, since_seq: i64, limit: i64) -> Result<Vec<LoggedAction>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT seq, payload FROM actions WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2")?;
        let rows = statement.query_map((since_seq, limit), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut logged = Vec::new();
        for row in rows {
            let (seq, payload) = row?;
            logged.push(LoggedAction {
                seq,
                action: serde_json::from_str(&payload)?,
            });
        }
        Ok(logged)
    }

    pub fn action_count(&self) -> Result<i64> {
        let connection = self.lock()?;
        Ok(connection.query_row("SELECT COUNT(*) FROM actions", [], |row| row.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember_core::{seed, ActionKind, GuestStatus, OrderStatus, TableStatus};

    fn action(id: &str, kind: ActionKind) -> Action {
        Action {
            id: id.into(),
            at: "2026-08-13T10:00:00.000Z".into(),
            actor: None,
            kind,
        }
    }

    fn seat(id: &str, guest_id: &str, table_id: &str) -> Action {
        action(
            id,
            ActionKind::SeatGuest {
                guest_id: guest_id.into(),
                table_id: table_id.into(),
            },
        )
    }

    fn changed(applied: Applied) -> Revision {
        match applied {
            Applied::Changed(revision) => revision,
            other => panic!("expected the action to be applied, got {other:?}"),
        }
    }

    #[test]
    fn a_new_store_starts_from_the_seeded_service() {
        let store = Store::in_memory().unwrap();
        let revision = store.revision().unwrap();

        assert_eq!(revision.version, 0);
        assert_eq!(revision.state, seed::initial_state());
        assert_eq!(store.action_count().unwrap(), 0);
    }

    #[test]
    fn applying_an_action_bumps_the_version_and_logs_it() {
        let store = Store::in_memory().unwrap();
        let revision = changed(store.apply(&seat("a1", "guest-maya", "t2")).unwrap());

        assert_eq!(revision.version, 1);
        assert_eq!(
            revision.state.table("t2").unwrap().status,
            TableStatus::Occupied
        );
        assert_eq!(store.action_count().unwrap(), 1);

        let logged = store.actions(0, 100).unwrap();
        assert_eq!(logged.len(), 1);
        assert_eq!(logged[0].action.id, "a1");
    }

    #[test]
    fn an_allowed_no_op_is_not_reported_as_a_refusal() {
        let store = Store::in_memory().unwrap();
        let existing = store
            .revision()
            .unwrap()
            .state
            .guest("guest-maya")
            .unwrap()
            .notes
            .clone();

        // Re-saving the notes that are already there is not an error, and must
        // not be shown to staff as one.
        assert_eq!(
            store
                .apply(&action(
                    "a1",
                    ActionKind::UpdateGuestNotes {
                        guest_id: "guest-maya".into(),
                        notes: existing,
                    }
                ))
                .unwrap(),
            Applied::Unchanged
        );
        assert_eq!(store.revision().unwrap().version, 0);
        assert_eq!(store.action_count().unwrap(), 0);
    }

    #[test]
    fn a_rejected_action_changes_nothing_and_is_not_logged() {
        let store = Store::in_memory().unwrap();
        // Jordan is "expected", not checked in — seating must be refused.
        assert_eq!(
            store.apply(&seat("a1", "guest-jordan", "t7")).unwrap(),
            Applied::Rejected(Rejection::GuestNotReadyToSeat),
            "the store must carry the reason back, not just the refusal"
        );

        assert_eq!(store.revision().unwrap().version, 0);
        assert_eq!(
            store.action_count().unwrap(),
            0,
            "a refused action must not pollute the audit log"
        );
    }

    #[test]
    fn replaying_the_same_action_id_is_a_no_op() {
        let store = Store::in_memory().unwrap();
        changed(store.apply(&seat("a1", "guest-maya", "t2")).unwrap());

        // Same id delivered twice — a retried request, or a reconnecting client
        // flushing its queue. It must not move the party a second time.
        assert_eq!(
            store.apply(&seat("a1", "guest-maya", "t2")).unwrap(),
            Applied::Duplicate
        );
        assert_eq!(store.revision().unwrap().version, 1);
        assert_eq!(store.action_count().unwrap(), 1);
    }

    /// A database path nothing else is using.
    ///
    /// The `-wal` and `-shm` sidecars have to go too: SQLite replays a leftover
    /// write-ahead log into what looks like a fresh file, so removing only the
    /// database leaves the previous run's committed actions behind and makes
    /// these tests flaky.
    fn temp_database(name: &str) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("ember-store-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("{name}.db"));
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        path
    }

    #[test]
    fn a_fresh_database_is_at_the_current_schema_version() {
        let store = Store::in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as i64);
    }

    #[test]
    fn reopening_does_not_re_run_migrations() {
        let path = temp_database("migrate-idempotent");
        let first = Store::open(&path).unwrap();
        changed(first.apply(&seat("a1", "guest-maya", "t2")).unwrap());
        let version = first.schema_version().unwrap();
        drop(first);

        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.schema_version().unwrap(), version);
        // A re-run of migration 0 would have been harmless, but a re-run of a
        // future data migration would not: the guard is that it never runs.
        assert_eq!(reopened.action_count().unwrap(), 1);
        assert_eq!(reopened.revision().unwrap().version, 1);
    }

    #[test]
    fn a_database_from_before_the_migration_runner_upgrades_in_place() {
        // Databases in the field were provisioned by a bare `execute_batch` and
        // so sit at user_version 0 with the tables already present. Opening one
        // must adopt it, not fail on "table already exists" and not re-seed
        // over the service it holds.
        let path = temp_database("migrate-legacy");
        {
            let legacy = rusqlite::Connection::open(&path).unwrap();
            legacy.execute_batch(MIGRATIONS[0].sql).unwrap();
            legacy
                .execute(
                    "INSERT INTO snapshot (id, version, state) VALUES (1, 7, ?1)",
                    [serde_json::to_string(&ember_core::seed::initial_state()).unwrap()],
                )
                .unwrap();
            assert_eq!(
                legacy
                    .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
                    .unwrap(),
                0,
                "the fixture must look like a pre-runner database"
            );
        }

        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(
            store.revision().unwrap().version,
            7,
            "the existing service must survive the upgrade"
        );
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused() {
        let path = temp_database("migrate-future");
        {
            let store = Store::open(&path).unwrap();
            changed(store.apply(&seat("a1", "guest-maya", "t2")).unwrap());
        }
        {
            let connection = rusqlite::Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "user_version", 999_i64)
                .unwrap();
        }

        // Opening it read-write would write today's shape over a schema this
        // build cannot see, so it has to fail loudly instead.
        match Store::open(&path) {
            Err(StoreError::FutureSchema { found, known }) => {
                assert_eq!(found, 999);
                assert_eq!(known, MIGRATIONS.len() as i64);
            }
            Ok(_) => panic!("expected opening a newer database to be refused"),
            Err(other) => panic!("expected a FutureSchema error, got {other:?}"),
        }
    }

    #[test]
    fn state_survives_reopening_the_database() {
        let path = temp_database("restart");

        {
            let store = Store::open(&path).unwrap();
            changed(store.apply(&seat("a1", "guest-maya", "t2")).unwrap());
            changed(
                store
                    .apply(&action(
                        "a2",
                        ActionKind::AddOrderItem {
                            guest_id: "guest-maya".into(),
                            menu_item_id: "beet-salad".into(),
                        },
                    ))
                    .unwrap(),
            );
        }

        let reopened = Store::open(&path).unwrap();
        let revision = reopened.revision().unwrap();
        assert_eq!(revision.version, 2, "version must survive a restart");
        assert_eq!(
            revision
                .state
                .table("t2")
                .unwrap()
                .seated_guest_id
                .as_deref(),
            Some("guest-maya")
        );
        assert_eq!(
            revision
                .state
                .order_for_guest("guest-maya")
                .unwrap()
                .lines
                .len(),
            1
        );
        assert_eq!(reopened.action_count().unwrap(), 2);
        // Deliberately no remove_dir_all here: the directory is shared by every
        // test in this binary, and tearing it down raced the other on-disk
        // tests into using a database that had just been deleted underneath
        // them. `temp_database` clears its own files on the way in instead.
    }

    #[test]
    fn the_log_records_every_step_of_a_service_in_order() {
        let store = Store::in_memory().unwrap();
        changed(store.apply(&seat("a1", "guest-maya", "t2")).unwrap());
        changed(
            store
                .apply(&action(
                    "a2",
                    ActionKind::AddOrderItem {
                        guest_id: "guest-maya".into(),
                        menu_item_id: "beet-salad".into(),
                    },
                ))
                .unwrap(),
        );
        let sent = changed(
            store
                .apply(&action(
                    "a3",
                    ActionKind::SendOrder {
                        guest_id: "guest-maya".into(),
                    },
                ))
                .unwrap(),
        );

        assert_eq!(
            sent.state.guest("guest-maya").unwrap().status,
            GuestStatus::Ordered
        );
        assert_eq!(
            sent.state.order_for_guest("guest-maya").unwrap().status,
            OrderStatus::Sent
        );

        let kinds: Vec<_> = store
            .actions(0, 100)
            .unwrap()
            .iter()
            .map(|entry| entry.action.kind.label())
            .collect();
        assert_eq!(kinds, ["seat-guest", "add-order-item", "send-order"]);
    }

    #[test]
    fn a_snapshot_written_before_stock_was_tracked_still_loads() {
        // Snapshots are persisted JSON, so every field added to PosState is a
        // migration. This is the shape the store wrote before `ingredients`
        // existed; without a default it fails with "missing field".
        let legacy = serde_json::json!({
            "tables": [],
            "guests": [],
            "orders": [],
            "activity": []
        })
        .to_string();

        let store = Store::in_memory().unwrap();
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE snapshot SET version = 7, state = ?1 WHERE id = 1",
                    [&legacy],
                )
                .unwrap();
        }

        let revision = store.revision().expect("a legacy snapshot must still load");
        assert_eq!(revision.version, 7);
        assert_eq!(
            revision.state.ingredients.len(),
            seed::ingredients().len(),
            "a service saved before stock was tracked starts from a full larder"
        );
    }

    #[test]
    fn an_order_saved_before_prices_were_recorded_on_the_line_still_loads() {
        // Lines used to be just an id, a quantity and a note. Without serde
        // defaults on the two new fields this fails outright with "missing
        // field", taking the whole service down on upgrade.
        let legacy = serde_json::json!({
            "tables": [],
            "guests": [],
            "orders": [{
                "id": "order-noah",
                "guestId": "guest-noah",
                "tableId": "t3",
                "status": "draft",
                "lines": [{ "menuItemId": "beet-salad", "quantity": 2, "notes": "" }],
                "guestNotes": "",
                "createdAt": "2026-08-09T21:39:00.000Z",
                "sentAt": null,
                "completedAt": null
            }],
            "activity": []
        })
        .to_string();

        let store = Store::in_memory().unwrap();
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE snapshot SET version = 4, state = ?1 WHERE id = 1",
                    [&legacy],
                )
                .unwrap();
        }

        let revision = store.revision().expect("a pre-price snapshot must load");
        let line = &revision.state.orders[0].lines[0];
        assert_eq!(line.menu_item_id, "beet-salad");
        assert_eq!(
            line.unit_price_cents, None,
            "no price was recorded for this line, and pretending otherwise \
             would invent a number nobody agreed"
        );
        assert_eq!(line.name_snapshot, None);

        // With nothing recorded, the menu is the only information there is —
        // which is how this line was totalled when it was written.
        assert_eq!(
            ember_core::order_total(revision.state.orders.first(), &seed::menu_items()),
            3400
        );
    }

    #[test]
    fn the_log_can_be_read_incrementally() {
        let store = Store::in_memory().unwrap();
        changed(store.apply(&seat("a1", "guest-maya", "t2")).unwrap());
        changed(store.apply(&seat("a2", "guest-priya", "t9")).unwrap());

        let first = store.actions(0, 1).unwrap();
        assert_eq!(first.len(), 1);

        let rest = store.actions(first[0].seq, 100).unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].action.id, "a2");
    }
}
