//! Durable state for Ember POS.
//!
//! Two tables. `actions` is an append-only log of everything that happened
//! during service; `snapshot` holds the reduced state so start-up does not have
//! to replay the log. The log is the interesting half — it is both the audit
//! trail and the training data the Python services will later learn from, which
//! is why nothing in here ever updates or deletes a row in it.

use std::path::Path;
use std::sync::Mutex;

use ember_core::{reduce, Action, PosState};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not encode or decode stored state: {0}")]
    Json(#[from] serde_json::Error),
    #[error("store mutex was poisoned")]
    Poisoned,
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
    /// A guard rejected the action, or it would not have changed anything.
    Rejected,
    /// This action id was already applied. Re-submitting is a no-op, so that a
    /// retried request cannot seat a party or fire an order twice.
    Duplicate,
}

const SCHEMA: &str = r#"
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
"#;

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

    fn from_connection(connection: rusqlite::Connection) -> Result<Self> {
        // WAL keeps a reader (the SSE snapshot fetch) from blocking a writer.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.execute_batch(SCHEMA)?;

        let store = Self {
            connection: Mutex::new(connection),
        };
        store.seed_if_empty()?;
        Ok(store)
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
        let Some(next_state) = reduce(&current.state, action) else {
            return Ok(Applied::Rejected);
        };
        let version = current.version + 1;

        transaction.execute(
            "INSERT INTO actions (id, at, kind, payload) VALUES (?1, ?2, ?3, ?4)",
            (
                &action.id,
                &action.at,
                action.kind.label(),
                serde_json::to_string(action)?,
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
        let mut statement = connection.prepare(
            "SELECT seq, payload FROM actions WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2",
        )?;
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
    fn a_rejected_action_changes_nothing_and_is_not_logged() {
        let store = Store::in_memory().unwrap();
        // Jordan is "expected", not checked in — seating must be refused.
        assert_eq!(
            store.apply(&seat("a1", "guest-jordan", "t7")).unwrap(),
            Applied::Rejected
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

    #[test]
    fn state_survives_reopening_the_database() {
        let directory = std::env::temp_dir().join(format!("ember-store-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("ember.db");
        let _ = std::fs::remove_file(&path);

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
            revision.state.table("t2").unwrap().seated_guest_id.as_deref(),
            Some("guest-maya")
        );
        assert_eq!(
            revision.state.order_for_guest("guest-maya").unwrap().lines.len(),
            1
        );
        assert_eq!(reopened.action_count().unwrap(), 2);

        std::fs::remove_dir_all(&directory).ok();
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
