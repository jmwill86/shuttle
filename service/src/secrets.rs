use regex::Regex;
use lazy_static::lazy_static;
use async_trait::async_trait;
use sqlx::{Executor, Execute, Row, Database, IntoArguments, Decode, ColumnIndex, Postgres, Type};
use sqlx::postgres::PgArguments;
use sqlx::database::HasArguments;
use sqlx::query::Query;

fn check_and_lower_secret_key(key: &str) -> Option<String> {
    lazy_static! {
        static ref VALID_KEY: Regex = Regex::new(r"[_a-zA-Z][_a-zA-Z0-9]*").unwrap();
    }
    VALID_KEY.is_match(key).then(|| key.to_lowercase())
}

/// Abstraction over a simple key/value 'secret' store. This may be used for any number of
/// purposes, such as storing API keys. Note that secrets are not encrypted and are stored directly
/// in a table in the database (meaning they can be accessed via SQL rather than this abstraction
/// should you prefer).
#[async_trait]
pub trait SecretStore<DB>
where
    DB: Database,
    for<'c> &'c Self: Executor<'c, Database = DB>,
    for<'c> <DB as HasArguments<'c>>::Arguments: IntoArguments<'c, DB>,
    for<'c> String: Decode<'c, DB> + Type<DB>,
    for<'c> usize: ColumnIndex<<DB as Database>::Row>,
    for<'c> Query<'c, Postgres, PgArguments>: Execute<'c, DB>
{
    // TODO: Don't restrict to Postgres types above.

    const GET_QUERY: &'static str;
    const SET_QUERY: &'static str;

    /// Read the secret with the given key from the database. Will return `None` if a secret with
    /// the given key does not exist or otherwise could not be accessed.
    async fn get_secret(&self, key: &str) -> Option<String> {
        let key = check_and_lower_secret_key(key)?;

        let query = sqlx::query(Self::GET_QUERY).bind(key);

        self.fetch_one(query).await.map(|row| row.get(0)).ok()
    }

    /// Create (or overwrite if already present) a key/value secret in the database. Will panic if
    /// the database could not be accessed or execution of the query otherwise failed.
    async fn set_secret(&self, key: &str, val: &str) {
        if let Some(key) = check_and_lower_secret_key(key) {
            let query = sqlx::query(Self::SET_QUERY).bind(key).bind(val);
            self.execute(query).await.unwrap();
        }
    }
}

#[async_trait]
impl SecretStore<sqlx::Postgres> for sqlx::PgPool {
    const GET_QUERY: &'static str = "SELECT value FROM secrets WHERE key = $1";
    const SET_QUERY: &'static str = "INSERT INTO secrets (key, value) VALUES ($1, $2)
                             ON CONFLICT (key) DO UPDATE SET value = $2";
}
