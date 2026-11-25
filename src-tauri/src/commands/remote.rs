//! Commands behind transcribing with another computer (the Models tab).

use crate::managers::remote::{RemoteEngineManager, RemoteServer, RemoteStatus};
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{self, TranscriptionEngine};
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Switching engines mid-dictation or mid-job would leave it half on each.
fn ensure_idle(app: &AppHandle, tm: &TranscriptionManager) -> Result<(), String> {
    if crate::tray::dictation_in_progress(app) || tm.job_active() {
        return Err("busy".to_string());
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_remote_status(manager: State<'_, Arc<RemoteEngineManager>>) -> RemoteStatus {
    manager.status()
}

/// Pairs with the computer a pasted code describes and selects it. Errors
/// are codes the UI has words for.
#[tauri::command]
#[specta::specta]
pub async fn pair_remote_computer(
    manager: State<'_, Arc<RemoteEngineManager>>,
    code: String,
) -> Result<RemoteServer, String> {
    let manager = Arc::clone(&manager);
    manager.pair(&code).await.map_err(|e| e.0.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn forget_remote_computer(
    app: AppHandle,
    manager: State<'_, Arc<RemoteEngineManager>>,
    tm: State<'_, Arc<TranscriptionManager>>,
    id: String,
) -> Result<(), String> {
    if settings::get_settings(&app).remote_server_id.as_deref() == Some(id.as_str()) {
        ensure_idle(&app, &tm)?;
    }
    manager.forget(&id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn rename_remote_computer(
    manager: State<'_, Arc<RemoteEngineManager>>,
    id: String,
    name: String,
) -> Result<(), String> {
    manager.rename(&id, &name).map_err(|e| e.to_string())
}

/// Transcribes with this paired computer from now on.
#[tauri::command]
#[specta::specta]
pub fn select_remote_computer(
    app: AppHandle,
    manager: State<'_, Arc<RemoteEngineManager>>,
    tm: State<'_, Arc<TranscriptionManager>>,
    id: String,
) -> Result<(), String> {
    ensure_idle(&app, &tm)?;
    if !manager
        .status()
        .servers
        .iter()
        .any(|server| server.id == id)
    {
        return Err("That computer isn't paired".to_string());
    }
    let mut settings = settings::get_settings(&app);
    settings.remote_server_id = Some(id);
    settings.transcription_engine = TranscriptionEngine::Remote;
    settings::write_settings(&app, settings);
    manager.apply_settings();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_transcription_engine_setting(
    app: AppHandle,
    manager: State<'_, Arc<RemoteEngineManager>>,
    tm: State<'_, Arc<TranscriptionManager>>,
    engine: TranscriptionEngine,
) -> Result<(), String> {
    ensure_idle(&app, &tm)?;
    let mut settings = settings::get_settings(&app);
    settings.transcription_engine = engine;
    if engine == TranscriptionEngine::Remote && settings.remote_server_id.is_none() {
        settings.remote_server_id = manager.status().servers.first().map(|s| s.id.clone());
    }
    settings::write_settings(&app, settings);
    manager.apply_settings();
    crate::tray::update_tray_menu(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_remote_fallback_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.remote_fallback = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// Tries to reach the selected computer now.
#[tauri::command]
#[specta::specta]
pub fn reconnect_remote_computer(manager: State<'_, Arc<RemoteEngineManager>>) {
    manager.reconnect();
}

/// Debug: act as if the other computer were unreachable.
#[tauri::command]
#[specta::specta]
pub fn set_remote_simulate_offline(manager: State<'_, Arc<RemoteEngineManager>>, enabled: bool) {
    manager.set_simulate_offline(enabled);
}
