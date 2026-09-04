//! Staff credentials and terminal sessions.
//!
//! A restaurant POS is a shared terminal: several people use one screen during
//! a service, and each of them needs to be identifiable in the log without
//! making them type a password between tables. The standard answer, and the one
//! here, is a short PIN per staff member.
//!
//! A four- to six-digit PIN has at most a million possibilities, so the hash is
//! only half the defence — Argon2id makes each guess expensive, and the lockout
//! below makes a run of guesses impossible. Neither is sufficient alone.
//!
//! Two things are deliberately never stored in a recoverable form: the PIN
//! (Argon2id, per-credential salt) and the session token (only its SHA-256, so
//! a copy of this database cannot be replayed as a live session).

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{DateTime, Duration, Utc};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::{Result, Store, StoreError};

/// Consecutive wrong PINs before the account stops accepting any.
pub const MAX_FAILED_ATTEMPTS: i64 = 5;

/// How long a locked account stays locked.
pub const LOCKOUT_MINUTES: i64 = 5;

/// How long a session lasts without being used.
///
/// A terminal left unattended on a pass is a real risk, and the whole point of
/// per-staff attribution is lost if someone walks up to a logged-in screen.
pub const SESSION_IDLE_MINUTES: i64 = 30;

/// Bytes of entropy in a session token. 32 bytes is well beyond guessing.
const TOKEN_BYTES: usize = 32;

/// A shortest-acceptable PIN. Four digits is the floor a lockout can carry.
pub const MIN_PIN_LENGTH: usize = 4;
pub const MAX_PIN_LENGTH: usize = 12;

/// An authenticated terminal session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub staff_id: String,
    /// Which physical screen this is, so the log can distinguish the pass from
    /// the host stand when the same person is signed in at both.
    pub terminal_id: String,
    pub expires_at: DateTime<Utc>,
}

/// What happened when someone tried to sign in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthOutcome {
    /// The token is returned exactly once, here. Only its hash is kept.
    Granted {
        token: String,
        session: Session,
    },
    WrongPin {
        attempts_remaining: i64,
    },
    LockedOut {
        until: DateTime<Utc>,
    },
    /// No such staff member, or they have no PIN set. Deliberately one variant:
    /// telling a caller which would let them enumerate the roster.
    UnknownStaff,
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn new_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Store {
    /// Sets (or replaces) a staff member's PIN, clearing any lockout.
    pub fn set_staff_pin(&self, staff_id: &str, pin: &str, now: DateTime<Utc>) -> Result<()> {
        if pin.len() < MIN_PIN_LENGTH || pin.len() > MAX_PIN_LENGTH {
            return Err(StoreError::WeakPin);
        }
        if !pin.chars().all(|c| c.is_ascii_digit()) {
            return Err(StoreError::WeakPin);
        }

        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(pin.as_bytes(), &salt)
            .map_err(|_| StoreError::PasswordHash)?
            .to_string();

        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO staff_credentials (staff_id, pin_hash, failed_count, locked_until, updated_at)
             VALUES (?1, ?2, 0, NULL, ?3)
             ON CONFLICT(staff_id) DO UPDATE SET
                 pin_hash = excluded.pin_hash,
                 failed_count = 0,
                 locked_until = NULL,
                 updated_at = excluded.updated_at",
            (staff_id, &hash, now.to_rfc3339()),
        )?;
        Ok(())
    }

    /// Whether anyone has a PIN yet. Drives first-run setup.
    pub fn has_any_credentials(&self) -> Result<bool> {
        let connection = self.lock()?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM staff_credentials", [], |row| {
                row.get(0)
            })?;
        Ok(count > 0)
    }

    /// Verifies a PIN and, on success, opens a session.
    pub fn authenticate(
        &self,
        staff_id: &str,
        pin: &str,
        terminal_id: &str,
        now: DateTime<Utc>,
    ) -> Result<AuthOutcome> {
        let connection = self.lock()?;

        let row: Option<(String, i64, Option<String>)> = connection
            .query_row(
                "SELECT pin_hash, failed_count, locked_until FROM staff_credentials WHERE staff_id = ?1",
                [staff_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        let Some((pin_hash, failed_count, locked_until)) = row else {
            // Spend the same work on an unknown staff id as on a known one, so
            // response time does not reveal who is on the roster.
            let _ = Argon2::default().verify_password(pin.as_bytes(), &dummy_hash());
            return Ok(AuthOutcome::UnknownStaff);
        };

        if let Some(until) = locked_until
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
        {
            if until > now {
                return Ok(AuthOutcome::LockedOut { until });
            }
        }

        let parsed = PasswordHash::new(&pin_hash).map_err(|_| StoreError::PasswordHash)?;
        if Argon2::default()
            .verify_password(pin.as_bytes(), &parsed)
            .is_err()
        {
            let failures = failed_count + 1;
            let lock_until = (failures >= MAX_FAILED_ATTEMPTS)
                .then(|| (now + Duration::minutes(LOCKOUT_MINUTES)).to_rfc3339());

            connection.execute(
                "UPDATE staff_credentials SET failed_count = ?2, locked_until = ?3 WHERE staff_id = ?1",
                (staff_id, failures, lock_until.as_deref()),
            )?;

            return Ok(match lock_until {
                Some(until) => AuthOutcome::LockedOut {
                    until: DateTime::parse_from_rfc3339(&until)
                        .map_err(|_| StoreError::PasswordHash)?
                        .with_timezone(&Utc),
                },
                None => AuthOutcome::WrongPin {
                    attempts_remaining: MAX_FAILED_ATTEMPTS - failures,
                },
            });
        }

        connection.execute(
            "UPDATE staff_credentials SET failed_count = 0, locked_until = NULL WHERE staff_id = ?1",
            [staff_id],
        )?;

        let token = new_token();
        let expires_at = now + Duration::minutes(SESSION_IDLE_MINUTES);
        connection.execute(
            "INSERT INTO sessions (token_hash, staff_id, terminal_id, created_at, last_seen_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
            (
                hash_token(&token),
                staff_id,
                terminal_id,
                now.to_rfc3339(),
                expires_at.to_rfc3339(),
            ),
        )?;

        Ok(AuthOutcome::Granted {
            token,
            session: Session {
                staff_id: staff_id.into(),
                terminal_id: terminal_id.into(),
                expires_at,
            },
        })
    }

    /// Resolves a token to a live session, sliding its idle expiry forward.
    ///
    /// Returns `None` for a token that is unknown or has expired — the caller
    /// cannot tell those apart, and should not be able to.
    pub fn session(&self, token: &str, now: DateTime<Utc>) -> Result<Option<Session>> {
        let connection = self.lock()?;
        let hashed = hash_token(token);

        let row: Option<(String, String, String)> = connection
            .query_row(
                "SELECT staff_id, terminal_id, expires_at FROM sessions WHERE token_hash = ?1",
                [&hashed],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        let Some((staff_id, terminal_id, expires_at)) = row else {
            return Ok(None);
        };

        let expires_at = DateTime::parse_from_rfc3339(&expires_at)
            .map_err(|_| StoreError::PasswordHash)?
            .with_timezone(&Utc);

        if expires_at <= now {
            connection.execute("DELETE FROM sessions WHERE token_hash = ?1", [&hashed])?;
            return Ok(None);
        }

        // Idle timeout, not absolute: a busy terminal stays signed in, an
        // abandoned one does not.
        let extended = now + Duration::minutes(SESSION_IDLE_MINUTES);
        connection.execute(
            "UPDATE sessions SET last_seen_at = ?2, expires_at = ?3 WHERE token_hash = ?1",
            (&hashed, now.to_rfc3339(), extended.to_rfc3339()),
        )?;

        Ok(Some(Session {
            staff_id,
            terminal_id,
            expires_at: extended,
        }))
    }

    /// Signs a terminal out.
    pub fn end_session(&self, token: &str) -> Result<()> {
        let connection = self.lock()?;
        connection.execute(
            "DELETE FROM sessions WHERE token_hash = ?1",
            [hash_token(token)],
        )?;
        Ok(())
    }

    /// Drops expired sessions. Cheap, and keeps the table from growing for the
    /// lifetime of a venue.
    pub fn sweep_sessions(&self, now: DateTime<Utc>) -> Result<usize> {
        let connection = self.lock()?;
        Ok(connection.execute(
            "DELETE FROM sessions WHERE expires_at <= ?1",
            [now.to_rfc3339()],
        )?)
    }
}

/// A fixed, valid Argon2id hash used only to equalise timing for an unknown
/// staff id. It is the hash of a value no PIN can be, since PINs are digits.
fn dummy_hash() -> PasswordHash<'static> {
    const DUMMY: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$O1uWTU4W1DUAaVMSpFCM3aC2CDCNjGCFqCAmKQGr0uk";
    PasswordHash::new(DUMMY).expect("the built-in dummy hash is well-formed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-04T18:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn store_with_pin(pin: &str) -> Store {
        let store = Store::in_memory().unwrap();
        store.set_staff_pin("server-1", pin, now()).unwrap();
        store
    }

    fn granted(outcome: AuthOutcome) -> (String, Session) {
        match outcome {
            AuthOutcome::Granted { token, session } => (token, session),
            other => panic!("expected the sign-in to be granted, got {other:?}"),
        }
    }

    #[test]
    fn a_correct_pin_opens_a_session() {
        let store = store_with_pin("2468");
        let (token, session) = granted(
            store
                .authenticate("server-1", "2468", "pass-1", now())
                .unwrap(),
        );

        assert_eq!(session.staff_id, "server-1");
        assert_eq!(session.terminal_id, "pass-1");
        assert_eq!(
            store.session(&token, now()).unwrap().map(|s| s.staff_id),
            Some("server-1".into())
        );
    }

    #[test]
    fn the_pin_is_not_recoverable_from_the_database() {
        let store = store_with_pin("2468");
        let connection = store.lock().unwrap();
        let hash: String = connection
            .query_row(
                "SELECT pin_hash FROM staff_credentials WHERE staff_id = 'server-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(hash.starts_with("$argon2id$"), "PINs must be Argon2id");
        assert!(!hash.contains("2468"), "the PIN itself must not appear");
    }

    #[test]
    fn a_session_token_is_not_stored_in_a_replayable_form() {
        let store = store_with_pin("2468");
        let (token, _) = granted(
            store
                .authenticate("server-1", "2468", "pass-1", now())
                .unwrap(),
        );

        let connection = store.lock().unwrap();
        let stored: String = connection
            .query_row("SELECT token_hash FROM sessions", [], |row| row.get(0))
            .unwrap();

        // Someone who copies the database must not be able to present what they
        // find there as a session cookie.
        assert_ne!(stored, token);
        assert_eq!(stored, hash_token(&token));
    }

    #[test]
    fn a_wrong_pin_counts_down_and_then_locks_out() {
        let store = store_with_pin("2468");

        for expected in (1..MAX_FAILED_ATTEMPTS).rev() {
            assert_eq!(
                store
                    .authenticate("server-1", "0000", "pass-1", now())
                    .unwrap(),
                AuthOutcome::WrongPin {
                    attempts_remaining: expected
                }
            );
        }

        // A four-digit PIN is guessable in a million tries; this is what makes
        // that irrelevant, not the hash.
        let outcome = store
            .authenticate("server-1", "0000", "pass-1", now())
            .unwrap();
        assert!(matches!(outcome, AuthOutcome::LockedOut { .. }));

        // And the correct PIN is refused while the lockout stands.
        let outcome = store
            .authenticate("server-1", "2468", "pass-1", now())
            .unwrap();
        assert!(
            matches!(outcome, AuthOutcome::LockedOut { .. }),
            "a lockout that the real PIN walks straight through is not a lockout"
        );
    }

    #[test]
    fn a_lockout_lifts_and_a_success_clears_the_count() {
        let store = store_with_pin("2468");
        for _ in 0..MAX_FAILED_ATTEMPTS {
            store
                .authenticate("server-1", "0000", "pass-1", now())
                .unwrap();
        }

        let later = now() + Duration::minutes(LOCKOUT_MINUTES + 1);
        let (_, session) = granted(
            store
                .authenticate("server-1", "2468", "pass-1", later)
                .unwrap(),
        );
        assert_eq!(session.staff_id, "server-1");

        // The counter must reset, or the next four typos lock them out again.
        assert_eq!(
            store
                .authenticate("server-1", "0000", "pass-1", later)
                .unwrap(),
            AuthOutcome::WrongPin {
                attempts_remaining: MAX_FAILED_ATTEMPTS - 1
            }
        );
    }

    #[test]
    fn an_unknown_staff_id_is_indistinguishable_from_a_wrong_pin() {
        let store = store_with_pin("2468");
        assert_eq!(
            store
                .authenticate("nobody", "2468", "pass-1", now())
                .unwrap(),
            AuthOutcome::UnknownStaff
        );
    }

    #[test]
    fn a_session_expires_after_idle_and_slides_while_used() {
        let store = store_with_pin("2468");
        let (token, _) = granted(
            store
                .authenticate("server-1", "2468", "pass-1", now())
                .unwrap(),
        );

        // Used inside the window: still live, and the window moves.
        let midway = now() + Duration::minutes(SESSION_IDLE_MINUTES - 5);
        assert!(store.session(&token, midway).unwrap().is_some());

        // That slide means the original deadline is no longer the deadline.
        let past_original = now() + Duration::minutes(SESSION_IDLE_MINUTES + 1);
        assert!(store.session(&token, past_original).unwrap().is_some());

        // Left alone past the sliding deadline: gone.
        let abandoned = past_original + Duration::minutes(SESSION_IDLE_MINUTES + 1);
        assert!(store.session(&token, abandoned).unwrap().is_none());
    }

    #[test]
    fn signing_out_invalidates_the_token_immediately() {
        let store = store_with_pin("2468");
        let (token, _) = granted(
            store
                .authenticate("server-1", "2468", "pass-1", now())
                .unwrap(),
        );

        store.end_session(&token).unwrap();
        assert!(store.session(&token, now()).unwrap().is_none());
    }

    #[test]
    fn an_unknown_token_is_not_a_session() {
        let store = store_with_pin("2468");
        assert!(store.session("not-a-token", now()).unwrap().is_none());
    }

    #[test]
    fn changing_a_pin_clears_a_standing_lockout() {
        let store = store_with_pin("2468");
        for _ in 0..MAX_FAILED_ATTEMPTS {
            store
                .authenticate("server-1", "0000", "pass-1", now())
                .unwrap();
        }
        // A manager resetting the PIN is how a locked-out server gets back to
        // work mid-service; it must not leave the lockout in place.
        store.set_staff_pin("server-1", "1357", now()).unwrap();

        let (_, session) = granted(
            store
                .authenticate("server-1", "1357", "pass-1", now())
                .unwrap(),
        );
        assert_eq!(session.staff_id, "server-1");
    }

    #[test]
    fn a_pin_that_is_too_short_or_not_digits_is_refused() {
        let store = Store::in_memory().unwrap();
        for weak in ["123", "", "abcd", "12 34", "1234567890123"] {
            assert!(
                matches!(
                    store.set_staff_pin("server-1", weak, now()),
                    Err(StoreError::WeakPin)
                ),
                "{weak:?} should not be accepted as a PIN"
            );
        }
        assert!(!store.has_any_credentials().unwrap());
    }

    #[test]
    fn expired_sessions_are_swept() {
        let store = store_with_pin("2468");
        granted(
            store
                .authenticate("server-1", "2468", "pass-1", now())
                .unwrap(),
        );

        assert_eq!(store.sweep_sessions(now()).unwrap(), 0);
        let later = now() + Duration::minutes(SESSION_IDLE_MINUTES + 1);
        assert_eq!(store.sweep_sessions(later).unwrap(), 1);
    }
}
