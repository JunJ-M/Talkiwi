mod migrations;
mod session_repo;

pub use session_repo::{SessionDetail, SessionRepo};

use rusqlite::Connection;

/// Initialize the database with WAL mode and run migrations.
pub fn init_database(path: &std::path::Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    migrations::run(&conn)?;
    Ok(conn)
}

/// Initialize an in-memory database for testing.
pub fn init_database_memory() -> anyhow::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    migrations::run(&conn)?;
    Ok(conn)
}
