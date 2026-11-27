//! Lógica compartida entre la app de Susurro y su servidor MCP:
//! almacenamiento (SQLite + FTS5), postprocesado de texto y fusión de pistas de reunión.

pub mod audio;
pub mod db;
pub mod echo;
pub mod meeting;
pub mod textproc;

pub use db::Db;

/// Ruta por defecto de la base de datos: `~/Library/Application Support/Susurro/susurro.db`.
pub fn default_db_path() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    home.join("Library/Application Support/Susurro/susurro.db")
}

/// Milisegundos desde epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
