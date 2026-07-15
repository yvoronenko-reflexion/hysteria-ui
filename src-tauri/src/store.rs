//! Profile persistence: a single `profiles.json` in the app config dir.

use crate::model::Profile;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

fn profiles_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("profiles.json"))
}

pub fn load(app: &AppHandle) -> Result<Vec<Profile>, String> {
    let path = profiles_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&data).map_err(|e| format!("Corrupt profiles.json: {e}"))
}

fn save_all(app: &AppHandle, profiles: &[Profile]) -> Result<(), String> {
    let path = profiles_path(app)?;
    let data = serde_json::to_string_pretty(profiles).map_err(|e| e.to_string())?;
    fs::write(&path, data).map_err(|e| e.to_string())
}

/// Insert or update a profile (matched by id). Returns the full updated list.
/// A profile with an empty id is treated as new and assigned a fresh uuid.
pub fn upsert(app: &AppHandle, mut profile: Profile) -> Result<Vec<Profile>, String> {
    let mut profiles = load(app)?;
    if profile.id.is_empty() {
        profile.id = Uuid::new_v4().to_string();
    }
    match profiles.iter_mut().find(|p| p.id == profile.id) {
        Some(existing) => *existing = profile,
        None => profiles.push(profile),
    }
    save_all(app, &profiles)?;
    Ok(profiles)
}

/// Delete a profile by id. Returns the full updated list.
pub fn delete(app: &AppHandle, id: &str) -> Result<Vec<Profile>, String> {
    let mut profiles = load(app)?;
    profiles.retain(|p| p.id != id);
    save_all(app, &profiles)?;
    Ok(profiles)
}
