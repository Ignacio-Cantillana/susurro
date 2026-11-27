//! Comandos que llama la interfaz React con `invoke`.

use std::sync::Arc;

use serde::Serialize;
use susurro_core::db::{DictTerm, Entry, EntryKind, Meeting, SearchHit, Segment, Snippet};
use susurro_core::meeting::{to_markdown, transcript};
use tauri::{AppHandle, Manager, State};

use crate::dictation::{self, Mode, Status};
use crate::meeting::{self, MeetingStatus};
use crate::models::{self, ModelInfo};
use crate::permissions::{self, Permissions};
use crate::settings::Settings;
use crate::{secrets, ui};

type St<'a> = State<'a, Arc<AppState>>;
type Res<T> = Result<T, String>;

use crate::state::AppState;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------- Estado y control ----------

#[derive(Serialize)]
pub struct FullStatus {
    dictation: Status,
    meeting: MeetingStatus,
}

#[tauri::command]
pub fn get_status(st: St) -> FullStatus {
    FullStatus { dictation: st.dictation.lock().status(), meeting: meeting::status(&st) }
}

#[tauri::command]
pub fn toggle_dictation(st: St, mode: Mode) {
    dictation::toggle(st.inner(), mode);
}

#[tauri::command]
pub fn cancel_dictation(st: St) {
    dictation::cancel(st.inner(), true);
}

#[tauri::command]
pub fn start_meeting(st: St, title: Option<String>) -> Res<i64> {
    meeting::start(st.inner(), title).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn stop_meeting(st: St) {
    meeting::stop(st.inner());
}

// ---------- Dictados y notas ----------

#[tauri::command]
pub fn list_entries(st: St, kind: EntryKind, limit: i64, offset: i64) -> Res<Vec<Entry>> {
    st.db.lock().list_entries(kind, limit, offset).map_err(err)
}

#[tauri::command]
pub fn update_entry(st: St, id: i64, text: String) -> Res<()> {
    st.db.lock().update_entry_text(id, &text).map_err(err)
}

#[tauri::command]
pub fn delete_entry(st: St, id: i64) -> Res<()> {
    st.db.lock().delete_entry(id).map_err(err)
}

#[tauri::command]
pub fn search(st: St, query: String) -> Res<Vec<SearchHit>> {
    st.db.lock().search(&query, 100).map_err(err)
}

#[tauri::command]
pub fn copy_text(text: String) -> Res<()> {
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text)).map_err(err)
}

// ---------- Reuniones ----------

#[derive(Serialize)]
pub struct MeetingDetail {
    meeting: Meeting,
    turns: Vec<Segment>,
}

#[tauri::command]
pub fn list_meetings(st: St, limit: i64, offset: i64) -> Res<Vec<Meeting>> {
    st.db.lock().list_meetings(limit, offset).map_err(err)
}

#[tauri::command]
pub fn get_meeting(st: St, id: i64) -> Res<Option<MeetingDetail>> {
    let db = st.db.lock();
    let dict = db.list_dictionary().map_err(err)?;
    let found = db.get_meeting(id).map_err(err)?;
    Ok(found.map(|(meeting, segs)| MeetingDetail { meeting, turns: transcript(segs, &dict) }))
}

#[tauri::command]
pub fn meeting_markdown(st: St, id: i64) -> Res<String> {
    let db = st.db.lock();
    let (m, segs) = db.get_meeting(id).map_err(err)?.ok_or("la reunión no existe")?;
    Ok(to_markdown(&m, &segs, &db.list_dictionary().map_err(err)?))
}

#[tauri::command]
pub fn rename_meeting(st: St, id: i64, title: String) -> Res<()> {
    st.db.lock().rename_meeting(id, &title).map_err(err)
}

#[tauri::command]
pub fn delete_meeting(st: St, id: i64) -> Res<()> {
    let audio = st.db.lock().get_meeting(id).map_err(err)?.and_then(|(m, _)| m.audio_path);
    st.db.lock().delete_meeting(id).map_err(err)?;
    // Solo borra audio dentro de la carpeta de reuniones de Susurro.
    if let Some(dir) = audio.map(std::path::PathBuf::from) {
        if dir.starts_with(meeting::meetings_dir()) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    Ok(())
}

// ---------- Diccionario y snippets ----------

#[tauri::command]
pub fn list_dictionary(st: St) -> Res<Vec<DictTerm>> {
    st.db.lock().list_dictionary().map_err(err)
}

#[tauri::command]
pub fn upsert_dictionary(st: St, term: String, sounds_like: String) -> Res<()> {
    if term.trim().is_empty() {
        return Err("la palabra no puede estar vacía".into());
    }
    st.db.lock().upsert_dictionary(&term, &sounds_like).map_err(err)
}

#[tauri::command]
pub fn delete_dictionary(st: St, id: i64) -> Res<()> {
    st.db.lock().delete_dictionary(id).map_err(err)
}

#[tauri::command]
pub fn list_snippets(st: St) -> Res<Vec<Snippet>> {
    st.db.lock().list_snippets().map_err(err)
}

#[tauri::command]
pub fn upsert_snippet(st: St, trigger: String, expansion: String) -> Res<()> {
    if trigger.trim().is_empty() || expansion.is_empty() {
        return Err("el disparador y el texto no pueden estar vacíos".into());
    }
    st.db.lock().upsert_snippet(&trigger, &expansion).map_err(err)
}

#[tauri::command]
pub fn delete_snippet(st: St, id: i64) -> Res<()> {
    st.db.lock().delete_snippet(id).map_err(err)
}

// ---------- Ajustes ----------

#[tauri::command]
pub fn get_settings(st: St) -> Settings {
    st.settings.read().clone()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, st: St, settings: Settings) -> Res<()> {
    settings.save(&st.db.lock()).map_err(err)?;
    crate::hotkey::set_hotkey(settings.hotkey);
    if let Some(m) = models::find(&settings.local_model) {
        st.local.set_model(models::path_of(m));
    }
    ui::set_widget_visible(&app, settings.show_widget);
    *st.settings.write() = settings;
    st.purge_old_dictations();
    Ok(())
}

#[tauri::command]
pub fn list_input_devices() -> Vec<String> {
    crate::audio::mic::list_input_devices()
}

#[tauri::command]
pub fn set_api_key(provider: String, key: String) -> Res<()> {
    secrets::set(&provider, &key).map_err(err)
}

#[tauri::command]
pub fn has_api_key(provider: String) -> bool {
    secrets::has(&provider)
}

#[tauri::command]
pub fn check_permissions() -> Permissions {
    permissions::check()
}

#[tauri::command]
pub fn request_permission(kind: String) {
    permissions::request(&kind);
}

// ---------- Modelos ----------

#[derive(Serialize)]
pub struct ModelState {
    #[serde(flatten)]
    info: ModelInfo,
    downloaded: bool,
}

#[tauri::command]
pub fn list_models() -> Vec<ModelState> {
    models::CATALOG.iter().map(|m| ModelState { info: m.clone(), downloaded: models::is_downloaded(m) }).collect()
}

#[derive(Clone, Serialize)]
struct Progress {
    id: String,
    done: u64,
    total: u64,
    error: Option<String>,
    finished: bool,
}

#[tauri::command]
pub fn download_model(st: St, id: String) -> Res<()> {
    let model = models::find(&id).ok_or("modelo desconocido")?;
    let st = st.inner().clone();
    std::thread::spawn(move || {
        let emit = |done, total, error: Option<String>, finished| {
            st.emit("model-progress", Progress { id: model.id.into(), done, total, error, finished });
        };
        match models::download(model, |d, t| emit(d, t, None, false)) {
            Ok(_) => {
                emit(1, 1, None, true);
                if st.settings.read().local_model == model.id {
                    let _ = st.local.warm_up();
                }
            }
            Err(e) => emit(0, 0, Some(format!("{e:#}")), true),
        }
    });
    Ok(())
}

#[tauri::command]
pub fn delete_model(st: St, id: String) -> Res<()> {
    let model = models::find(&id).ok_or("modelo desconocido")?;
    if st.settings.read().local_model == id {
        st.local.unload();
    }
    models::delete(model).map_err(err)
}

// ---------- Ventanas ----------

#[tauri::command]
pub fn show_main(app: AppHandle, page: Option<String>) {
    ui::show_main(&app, page.as_deref());
}

#[tauri::command]
pub fn widget_expand(app: AppHandle, mode: String) {
    ui::widget_expand(&app, &mode);
}

#[derive(Serialize)]
pub struct McpInfo {
    binary: String,
    command: String,
    exists: bool,
}

/// Ruta del binario `susurro-mcp` (va junto al ejecutable de la app) y el comando
/// para darlo de alta en Claude Code.
#[tauri::command]
pub fn mcp_info(app: AppHandle) -> McpInfo {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("susurro-mcp")),
        app.path().resource_dir().ok().map(|d| d.join("susurro-mcp")),
    ];
    let path = candidates.iter().flatten().find(|p| p.is_file()).cloned().or(candidates[0].clone());
    let binary = path.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    McpInfo {
        command: format!("claude mcp add --scope user susurro -- \"{binary}\""),
        exists: path.is_some_and(|p| p.is_file()),
        binary,
    }
}
