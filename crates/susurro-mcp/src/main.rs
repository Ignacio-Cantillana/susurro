//! Servidor MCP de Susurro (stdio). Da a Claude acceso de solo lectura a las
//! reuniones, dictados y notas guardados por la app.
//!
//! Alta en Claude Code:  claude mcp add --scope user susurro -- /ruta/a/susurro-mcp
//! Base de datos: `~/Library/Application Support/Susurro/susurro.db` o `SUSURRO_DB`.

use std::path::PathBuf;

use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use susurro_core::Db;
use susurro_core::db::EntryKind;
use susurro_core::meeting::{format_offset, to_markdown};

#[derive(Clone)]
struct Susurro {
    db_path: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
struct ListArgs {
    /// Cuántos elementos devolver (por defecto 20, máximo 200).
    limit: Option<i64>,
}

#[derive(Deserialize, JsonSchema)]
struct MeetingArgs {
    /// Id de la reunión (sale en `list_meetings`). Si se omite, la más reciente.
    id: Option<i64>,
}

#[derive(Deserialize, JsonSchema)]
struct SearchArgs {
    /// Palabras a buscar. Se buscan como prefijos y sin tener en cuenta acentos.
    query: String,
    /// Máximo de resultados (por defecto 30).
    limit: Option<i64>,
}

fn limit(l: Option<i64>, default: i64) -> i64 {
    l.unwrap_or(default).clamp(1, 200)
}

/// Fecha local legible a partir de milisegundos epoch, sin dependencias de fechas.
fn fmt_ts(ms: i64) -> String {
    std::process::Command::new("date")
        .args(["-r", &(ms / 1000).to_string(), "+%Y-%m-%d %H:%M"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| ms.to_string())
}

impl Susurro {
    fn db(&self) -> Result<Db, String> {
        Db::open_readonly(&self.db_path).map_err(|e| format!("{e:#}"))
    }
}

#[tool_router]
impl Susurro {
    fn new(db_path: PathBuf) -> Self {
        Self { db_path, tool_router: Self::tool_router() }
    }

    #[tool(description = "Lista las reuniones grabadas con Susurro, de la más reciente a la más antigua: id, título, fecha, duración y número de intervenciones.")]
    async fn list_meetings(&self, Parameters(args): Parameters<ListArgs>) -> Result<String, String> {
        let meetings = self.db()?.list_meetings(limit(args.limit, 20), 0).map_err(|e| e.to_string())?;
        if meetings.is_empty() {
            return Ok("No hay reuniones grabadas.".into());
        }
        Ok(meetings
            .iter()
            .map(|m| {
                let dur = m.ended_at.map(|e| format_offset(e - m.started_at)).unwrap_or_else(|| "en curso".into());
                format!("- id {} · {} · {} · {} · {} segmentos", m.id, m.title, fmt_ts(m.started_at), dur, m.segment_count)
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    #[tool(description = "Devuelve la transcripción completa de una reunión en Markdown, con hablante («Yo» = el usuario por su micrófono, «Otros» = el resto por el audio del sistema) y marca de tiempo. Sin id, la reunión más reciente.")]
    async fn get_meeting_transcript(&self, Parameters(args): Parameters<MeetingArgs>) -> Result<String, String> {
        let db = self.db()?;
        let id = match args.id {
            Some(id) => id,
            None => db.list_meetings(1, 0).map_err(|e| e.to_string())?.first().map(|m| m.id).ok_or("No hay reuniones grabadas.")?,
        };
        let (meeting, segments) = db.get_meeting(id).map_err(|e| e.to_string())?.ok_or(format!("No existe la reunión {id}."))?;
        let mut md = to_markdown(&meeting, &segments);
        md.insert_str(md.find('\n').map(|i| i + 1).unwrap_or(0), &format!("\nFecha: {}\n", fmt_ts(meeting.started_at)));
        Ok(md)
    }

    #[tool(description = "Búsqueda de texto completo en dictados, notas y reuniones. Devuelve fragmentos con la coincidencia entre «» y de dónde vienen.")]
    async fn search_transcripts(&self, Parameters(args): Parameters<SearchArgs>) -> Result<String, String> {
        let hits = self.db()?.search(&args.query, limit(args.limit, 30)).map_err(|e| e.to_string())?;
        if hits.is_empty() {
            return Ok(format!("Sin resultados para «{}».", args.query));
        }
        Ok(hits
            .iter()
            .map(|h| {
                let source = match (h.source.as_str(), &h.meeting_title) {
                    ("segment", Some(t)) => format!("reunión «{t}» (id {})", h.id),
                    ("note", _) => format!("nota {}", h.id),
                    _ => format!("dictado {}", h.id),
                };
                format!("- [{}] {}: {}", fmt_ts(h.created_at), source, h.snippet)
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    #[tool(description = "Últimos textos dictados por el usuario (lo que ha pegado en otras apps), con fecha y app de destino.")]
    async fn list_dictations(&self, Parameters(args): Parameters<ListArgs>) -> Result<String, String> {
        self.entries(EntryKind::Dictation, args.limit)
    }

    #[tool(description = "Últimas notas de voz del usuario (ideas que dictó para guardar, no para pegar).")]
    async fn list_notes(&self, Parameters(args): Parameters<ListArgs>) -> Result<String, String> {
        self.entries(EntryKind::Note, args.limit)
    }
}

impl Susurro {
    fn entries(&self, kind: EntryKind, l: Option<i64>) -> Result<String, String> {
        let entries = self.db()?.list_entries(kind, limit(l, 20), 0).map_err(|e| e.to_string())?;
        if entries.is_empty() {
            return Ok("No hay nada todavía.".into());
        }
        Ok(entries
            .iter()
            .map(|e| {
                let app = e.app.as_deref().map(|a| format!(" · {a}")).unwrap_or_default();
                format!("- [{}{app}] (id {}) {}", fmt_ts(e.created_at), e.id, e.text)
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Susurro {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("susurro", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Acceso de solo lectura a lo que el usuario ha transcrito con Susurro: reuniones (con «Yo» y «Otros»), \
                 dictados y notas de voz. Para preguntas sobre una reunión, usa list_meetings y luego \
                 get_meeting_transcript; para encontrar algo concreto, search_transcripts.",
            )
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let db_path = std::env::var_os("SUSURRO_DB").map(PathBuf::from).unwrap_or_else(susurro_core::default_db_path);
    let server = Susurro::new(db_path).serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
