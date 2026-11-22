//! Commands behind the Mobile settings tab.
//!
//! The ones that start or stop the server are deliberately not `async`: they
//! block on the runtime to bind the port, which a runtime thread can't do.

use crate::managers::mobile::{MobileLinkManager, MobilePairing, MobileStatus};
use crate::settings::{self, MobileOutputMode};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub fn get_mobile_status(manager: State<'_, Arc<MobileLinkManager>>) -> MobileStatus {
    manager.load_devices();
    manager.status()
}

#[tauri::command]
#[specta::specta]
pub fn change_mobile_enabled_setting(
    app: AppHandle,
    manager: State<'_, Arc<MobileLinkManager>>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.mobile_enabled = enabled;
    settings::write_settings(&app, settings);
    manager.apply_settings();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_mobile_port_setting(
    app: AppHandle,
    manager: State<'_, Arc<MobileLinkManager>>,
    port: u16,
) -> Result<(), String> {
    // Below 1024 needs privileges Handy doesn't run with.
    if port < 1024 {
        return Err("The port must be between 1024 and 65535".to_string());
    }
    let mut settings = settings::get_settings(&app);
    settings.mobile_port = port;
    settings::write_settings(&app, settings);
    manager.apply_settings();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_mobile_output_mode_setting(
    app: AppHandle,
    mode: MobileOutputMode,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.mobile_output_mode = mode;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_mobile_post_process_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.mobile_post_process = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn start_mobile_pairing(
    manager: State<'_, Arc<MobileLinkManager>>,
) -> Result<MobilePairing, String> {
    manager.start_pairing().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn cancel_mobile_pairing(manager: State<'_, Arc<MobileLinkManager>>) {
    manager.cancel_pairing();
}

#[tauri::command]
#[specta::specta]
pub fn rename_mobile_device(
    manager: State<'_, Arc<MobileLinkManager>>,
    id: String,
    name: String,
) -> Result<(), String> {
    manager.rename_device(&id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn remove_mobile_device(
    manager: State<'_, Arc<MobileLinkManager>>,
    id: String,
) -> Result<(), String> {
    manager.remove_device(&id).map_err(|e| e.to_string())
}
