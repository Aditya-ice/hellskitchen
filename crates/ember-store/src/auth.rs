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
        // Spent: there is exactly one first PIN, and this is it.
        connection.execute("DELETE FROM bootstrap", [])?;
        // A manager resetting the PIN is how a locked-out colleague gets back
        // on the floor mid-service, so it has to clear the lockout too. The
        // counter lives in its own table now, so clearing the credential row
        // no longer does this on its own.
        connection.execute("DELETE FROM login_attempts WHERE staff_id = ?1", [staff_id])?;
        // The urgent reason to change a PIN is that someone saw it typed. That
        // is exactly the case where the sessions it already opened must die
        // with it, rather than idling out over the next half hour.
        connection.execute("DELETE FROM sessions WHERE staff_id = ?1", [staff_id])?;
        connection.execute(
            "INSERT INTO staff_credentials (staff_id, pin_hash, failed_count, locked_until, updated_at)
             VALUES (?1, ?2, 0, NULL, ?3)
             ON CONFLICT(staff_id) DO UPDATE SET
                 pin_hash = excluded.pin_hash,
                 updated_at = excluded.updated_at",
            (staff_id, &hash, now.to_rfc3339()),
        )?;
        Ok(())
    }

    /// The one-time secret that authorises claiming the first PIN.
    ///
    /// `None` once any credential exists. Minted on first request and printed
    /// to the server console at startup, so the person standing at the machine
    /// is the one who can claim it — before this, the first client to reach the
    /// port on a freshly deployed venue network simply became the manager.
    pub fn bootstrap_token(&self) -> Result<Option<String>> {
        if self.has_any_credentials()? {
            return Ok(None);
        }

        let connection = self.lock()?;
        let existing: Option<String> = connection
            .query_row("SELECT token FROM bootstrap WHERE id = 1", [], |row| {
                row.get(0)
            })
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        if let Some(token) = existing {
            return Ok(Some(token));
        }

        let token = new_token();
        connection.execute("INSERT INTO bootstrap (id, token) VALUES (1, ?1)", [&token])?;
        Ok(Some(token))
    }

    /// Pins the bootstrap token to a known value.
    ///
    /// For a deployment that provisions the first manager from a script rather
    /// than by reading a console. Ignored once any credential exists, so it
    /// cannot be used to re-open setup on a running venue.
    pub fn set_bootstrap_token(&self, token: &str) -> Result<()> {
        if self.has_any_credentials()? {
            return Ok(());
        }
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO bootstrap (id, token) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET token = excluded.token",
            [token],
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
    ///
    /// Failed attempts are counted against the *attempted identifier*, whether
    /// or not anyone by that name exists. That is what makes an invented id
    /// indistinguishable from a real one: both count down, both lock, and both
    /// answer the same way at every step. Keeping the counter on the credential
    /// row meant only a real id could ever lock, so six wrong guesses told an
    /// attacker exactly which identifiers were on the roster.
    pub fn authenticate(
        &self,
        staff_id: &str,
        pin: &str,
        terminal_id: &str,
        now: DateTime<Utc>,
    ) -> Result<AuthOutcome> {
        let connection = self.lock()?;

        let attempts: Option<(i64, Option<String>)> = connection
            .query_row(
                "SELECT failed_count, locked_until FROM login_attempts WHERE staff_id = ?1",
                [staff_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;
        let (mut failed_count, locked_until) = attempts.unwrap_or((0, None));

        if let Some(until) = locked_until
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc))
        {
            if until > now {
                return Ok(AuthOutcome::LockedOut { until });
            }
            // The lockout has lapsed. Clear the count that caused it, or the
            // next single typo reaches the threshold again and re-locks
            // immediately, with no warning -- repeatable indefinitely, so one
            // mistake earlier in the night keeps costing someone the floor.
            connection.execute(
                "UPDATE login_attempts SET failed_count = 0, locked_until = NULL \
                 WHERE staff_id = ?1",
                [staff_id],
            )?;
            failed_count = 0;
        }

        let stored: Option<String> = connection
            .query_row(
                "SELECT pin_hash FROM staff_credentials WHERE staff_id = ?1",
                [staff_id],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?;

        // Spend the same work on an unknown id as on a known one, so response
        // time does not reveal who is on the roster either.
        let verified = match stored.as_deref() {
            Some(hash) => {
                let parsed = PasswordHash::new(hash).map_err(|_| StoreError::PasswordHash)?;
                Argon2::default()
                    .verify_password(pin.as_bytes(), &parsed)
                    .is_ok()
            }
            None => {
                let _ = Argon2::default().verify_password(pin.as_bytes(), &dummy_hash());
                false
            }
        };

        if !verified {
            let failures = failed_count + 1;
            let lock_until = (failures >= MAX_FAILED_ATTEMPTS)
                .then(|| (now + Duration::minutes(LOCKOUT_MINUTES)).to_rfc3339());

            connection.execute(
                "INSERT INTO login_attempts (staff_id, failed_count, locked_until) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT(staff_id) DO UPDATE SET \
                     failed_count = excluded.failed_count, \
                     locked_until = excluded.locked_until",
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

        connection.execute("DELETE FROM login_attempts WHERE staff_id = ?1", [staff_id])?;

        // Opportunistic, and the natural moment for it: sign-in is rare, and
        // without this the table gains a permanent row per session for the life
        // of the venue. An expired row is only ever deleted by presenting that
        // same token again, which an abandoned terminal by definition never
        // does.
        connection.execute(
            "DELETE FROM sessions WHERE expires_at <= ?1",
            [now.to_rfc3339()],
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

    /// Resolves a token **without** sliding its idle window.
    ///
    /// `session` renews on every read, which is right for a request — using the
    /// terminal is what keeps it signed in. It is wrong for anything that polls
    /// on a timer: the open event stream re-checked its own session every
    /// minute, and each check pushed the expiry another thirty minutes out, so
    /// a screen abandoned on the pass stayed subscribed to guest names and
    /// allergies forever. That is the exact case the idle expiry exists for.
    pub fn session_peek(&self, token: &str, now: DateTime<Utc>) -> Result<Option<Session>> {
        let connection = self.lock()?;
        let row: Option<(String, String, String)> = connection
            .query_row(
                "SELECT staff_id, terminal_id, expires_at FROM sessions WHERE token_hash = ?1",
                [hash_token(token)],
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
            return Ok(None);
        }
        Ok(Some(Session {
            staff_id,
            terminal_id,
            expires_at,
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
    fn a_lapsed_lockout_does_not_re_lock_on_the_very_next_typo() {
        let store = store_with_pin("2468");
        for _ in 0..MAX_FAILED_ATTEMPTS {
            store
                .authenticate("server-1", "0000", "pass-1", now())
                .unwrap();
        }

        let later = now() + Duration::minutes(LOCKOUT_MINUTES + 1);

        // The count that caused the lockout has to be cleared with it.
        // Otherwise this single typo is failure number six, trips the
        // threshold again straight away, and takes the server off the floor
        // for another five minutes with no warning -- repeatable forever.
        assert_eq!(
            store
                .authenticate("server-1", "0000", "pass-1", later)
                .unwrap(),
            AuthOutcome::WrongPin {
                attempts_remaining: MAX_FAILED_ATTEMPTS - 1
            }
        );

        // And the real PIN still works right after that typo.
        let (_, session) = granted(
            store
                .authenticate("server-1", "2468", "pass-1", later)
                .unwrap(),
        );
        assert_eq!(session.staff_id, "server-1");
    }

    #[test]
    fn changing_a_pin_revokes_the_sessions_it_opened() {
        let store = store_with_pin("2468");
        let (token, _) = granted(
            store
                .authenticate("server-1", "2468", "pass-1", now())
                .unwrap(),
        );
        assert!(store.session(&token, now()).unwrap().is_some());

        // A PIN is changed in a hurry because someone watched it being typed.
        // Leaving the session it already opened alive for another half hour
        // defeats the point of changing it.
        store.set_staff_pin("server-1", "1357", now()).unwrap();
        assert!(store.session(&token, now()).unwrap().is_none());
    }

    #[test]
    fn signing_in_clears_out_expired_sessions() {
        let store = store_with_pin("2468");
        granted(
            store
                .authenticate("server-1", "2468", "pass-1", now())
                .unwrap(),
        );

        let later = now() + Duration::minutes(SESSION_IDLE_MINUTES + 1);
        granted(
            store
                .authenticate("server-1", "2468", "pass-2", later)
                .unwrap(),
        );

        // Only the live one is left: an expired row is otherwise deleted only
        // by presenting that same token again, which an abandoned terminal
        // never does, so the table would grow for the life of the venue.
        let connection = store.lock().unwrap();
        let rows: i64 = connection
            .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn an_unknown_id_counts_down_and_locks_exactly_like_a_real_one() {
        let store = store_with_pin("2468");

        // Same countdown, same wording, same shape. Previously an invented id
        // returned a distinct outcome and could never lock, so six guesses
        // sorted the roster from the rest.
        for expected in (1..MAX_FAILED_ATTEMPTS).rev() {
            assert_eq!(
                store
                    .authenticate("not-a-person", "0000", "pass-1", now())
                    .unwrap(),
                AuthOutcome::WrongPin {
                    attempts_remaining: expected
                }
            );
        }
        let invented = store
            .authenticate("not-a-person", "0000", "pass-1", now())
            .unwrap();
        assert!(matches!(invented, AuthOutcome::LockedOut { .. }));

        // And a real id behaves identically at every step.
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
        let real = store
            .authenticate("server-1", "0000", "pass-1", now())
            .unwrap();
        assert!(matches!(real, AuthOutcome::LockedOut { .. }));
    }

    #[test]
    fn a_bootstrap_token_exists_only_until_the_first_pin_is_set() {
        let store = Store::in_memory().unwrap();

        let token = store
            .bootstrap_token()
            .unwrap()
            .expect("a fresh terminal offers one");
        assert_eq!(token.len(), 64, "expected a 32-byte token in hex");
        // Stable across reads, so the console value stays valid.
        assert_eq!(
            store.bootstrap_token().unwrap().as_deref(),
            Some(&token[..])
        );

        store.set_staff_pin("manager-1", "246810", now()).unwrap();

        // Spent. Without this, anyone who saw the console once could claim a
        // second "first" PIN after a credential was wiped.
        assert_eq!(store.bootstrap_token().unwrap(), None);
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
    fn peeking_at_a_session_does_not_keep_it_alive() {
        let store = store_with_pin("2468");
        let (token, _) = granted(
            store
                .authenticate("server-1", "2468", "pass-1", now())
                .unwrap(),
        );

        // A poller that renewed on every look would keep an abandoned terminal
        // signed in forever, which is what the event stream was doing.
        // Exclusive: at exactly SESSION_IDLE_MINUTES the session is already over.
        for minute in 1..SESSION_IDLE_MINUTES {
            let later = now() + Duration::minutes(minute);
            assert!(store.session_peek(&token, later).unwrap().is_some());
        }

        let expired = now() + Duration::minutes(SESSION_IDLE_MINUTES + 1);
        assert!(
            store.session_peek(&token, expired).unwrap().is_none(),
            "peeking must not have pushed the expiry out"
        );
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
