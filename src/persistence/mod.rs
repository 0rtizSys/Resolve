//! Local storage. Implements [`crate::core::Store`].

mod sqlite;

use std::path::PathBuf;

pub use sqlite::SqliteStore;

/// Environment variable that overrides where the database lives.
pub const DATA_DIR_ENV: &str = "RESOLVE_DATA_DIR";

/// Location of the database: `$RESOLVE_DATA_DIR/resolve.db`, or the platform data directory
/// (e.g. `~/.local/share/resolve` on Linux, `%APPDATA%\Resolve` on Windows).
pub fn default_database_path() -> Option<PathBuf> {
    let dir = match std::env::var_os(DATA_DIR_ENV) {
        Some(dir) => PathBuf::from(dir),
        None => directories::ProjectDirs::from("", "", "Resolve")?
            .data_dir()
            .to_path_buf(),
    };
    Some(dir.join("resolve.db"))
}
