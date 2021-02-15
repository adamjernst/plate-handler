use log::error;
use rusqlite::{Connection, Result, NO_PARAMS};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn open() -> Connection {
    let conn = Connection::open("/data/plates.db").expect("Unable to open db");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS plate (id TEXT NOT NULL PRIMARY KEY, name TEXT) WITHOUT ROWID",
        NO_PARAMS,
    )
    .expect("Unable to create plate table");
    conn.execute("CREATE TABLE IF NOT EXISTS spotting (plate_id TEXT NOT NULL, timestamp INTEGER NOT NULL, FOREIGN KEY(plate_id) REFERENCES plate(id))", NO_PARAMS).expect("Unable to create spotting table");
    conn
}

/// Inserts a row into 'spotting'; returns name, if any.
pub fn handle_spotted_plate(conn: &Connection, plate: &str) -> Option<String> {
    match handle_spotted_plate_impl(conn, plate) {
        Ok(x) => x,
        Err(e) => {
            error!("Error updating database for spotted plate: {:?}", e);
            None
        }
    }
}

fn handle_spotted_plate_impl(conn: &Connection, plate: &str) -> Result<Option<String>> {
    conn.execute("INSERT OR IGNORE INTO plate(id) VALUES (?1)", &[&plate])?;
    conn.execute_named(
        "INSERT OR IGNORE INTO spotting(plate_id, timestamp) VALUES (:plate, :timestamp)",
        &[
            (":plate", &plate),
            (
                ":timestamp",
                &SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64(),
            ),
        ],
    )?;

    let mut stmt = conn.prepare("SELECT name FROM plate WHERE id = ?1")?;
    let mut rows = stmt.query(&[&plate])?;
    if let Some(row) = rows.next()? {
        return Ok(row.get(0)?);
    }
    Ok(None)
}

pub fn update_name(conn: &Connection, plate: &str, name: &str) {
    if let Err(e) = conn.execute("UPDATE plate SET name = ?1 WHERE id = ?2", &[&name, &plate]) {
        error!(
            "Unable to set name to '{}' for plate '{}': {:?}",
            &name, &plate, e
        );
    }
}
