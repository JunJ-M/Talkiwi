use rusqlite::Connection;

const INIT_SQL: &str = include_str!("001_init.sql");

pub fn run(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(INIT_SQL)?;
    Ok(())
}
