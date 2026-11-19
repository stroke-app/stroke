//! Labs, several connections at once. A switch sets the open pool aside
//! instead of closing it; switching back takes it out again, so returning to a
//! database costs nothing. Only the pool handle moves (it is an Arc inside),
//! so this never touches the network.

use super::connection::{close_in_background, Db};

/// More than this many set-aside pools and the oldest is closed.
const MAX_PARKED: usize = 8;

/// Set the open connection aside under `key`. Nothing open: nothing to do.
#[tauri::command]
pub fn park_connection(state: Db<'_>, key: String) -> Result<(), String> {
    let Some(conn) = state.conn.lock().map_err(|e| e.to_string())?.take() else {
        return Ok(());
    };
    *state.active_key.lock().map_err(|e| e.to_string())? = None;
    let mut parked = state.parked.lock().map_err(|e| e.to_string())?;
    parked.retain(|(k, _)| k != &key);
    parked.push((key, conn));
    while parked.len() > MAX_PARKED {
        let (_, old) = parked.remove(0);
        close_in_background(old);
    }
    Ok(())
}

/// Make the pool set aside under `key` the open one. False when there is none
/// (the page then connects as usual).
#[tauri::command]
pub fn resume_connection(state: Db<'_>, key: String) -> Result<bool, String> {
    let found = {
        let mut parked = state.parked.lock().map_err(|e| e.to_string())?;
        parked.iter().position(|(k, _)| k == &key).map(|i| parked.remove(i).1)
    };
    let Some(conn) = found else { return Ok(false) };
    let previous = state.conn.lock().map_err(|e| e.to_string())?.replace(conn);
    *state.active_key.lock().map_err(|e| e.to_string())? = Some(key);
    if let Some(prev) = previous {
        close_in_background(prev);
    }
    Ok(true)
}

/// Close set-aside pools: one by key, every database of one saved connection
/// (`connection`, its id: it was edited or deleted, so a kept pool would point
/// at the old server or login), or all of them (Labs turned off).
#[tauri::command]
pub fn close_parked(
    state: Db<'_>,
    key: Option<String>,
    connection: Option<String>,
) -> Result<(), String> {
    let prefix = connection.map(|id| format!("{id}|"));
    let mut parked = state.parked.lock().map_err(|e| e.to_string())?;
    let (gone, kept): (Vec<_>, Vec<_>) = parked.drain(..).partition(|(k, _)| match (&key, &prefix) {
        (Some(want), _) => want == k,
        (None, Some(p)) => k.starts_with(p.as_str()),
        (None, None) => true,
    });
    *parked = kept;
    for (_, c) in gone {
        close_in_background(c);
    }
    Ok(())
}

/// Which connections are set aside, oldest first.
#[tauri::command]
pub fn parked_connections(state: Db<'_>) -> Result<Vec<String>, String> {
    Ok(state.parked.lock().map_err(|e| e.to_string())?.iter().map(|(k, _)| k.clone()).collect())
}

/// The page's key for the pool now in front, after it connects fresh (a
/// resume sets it itself). Routed calls compare against it.
#[tauri::command]
pub fn set_active_conn_key(state: Db<'_>, key: Option<String>) -> Result<(), String> {
    *state.active_key.lock().map_err(|e| e.to_string())? = key.filter(|k| !k.is_empty());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::connection::ActiveConnection;
    use super::*;

    /// park/resume/close_parked are sync commands: they run on a thread with
    /// no Tokio runtime. Closing a pool there used to call tokio::spawn, which
    /// panics outside a runtime, and release builds abort on a panic: turning
    /// Labs off or disconnecting closed the app.
    #[test]
    fn closing_a_pool_outside_a_runtime_does_not_panic() {
        let pool = tauri::async_runtime::block_on(sqlx::SqlitePool::connect("sqlite::memory:")).unwrap();
        let handle = std::thread::spawn(move || close_in_background(ActiveConnection::Sqlite(pool)));
        handle.join().expect("close_in_background panicked outside a runtime");
    }
}
