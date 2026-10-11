//! Tool status and installation (contracts/ipc.md, Accounts & settings).

use crate::error::AppResult;
use crate::state::AppState;
use crate::tools::manager::ToolsStatus;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
#[specta::specta]
pub async fn tools_status(state: State<'_, AppState>) -> AppResult<ToolsStatus> {
    Ok(state.tool_manager.status(state.tools.as_ref()).await)
}

/// Install missing tools, emitting `tools://status` with a progress message per step.
#[tauri::command]
#[specta::specta]
pub async fn install_tools(app: AppHandle, state: State<'_, AppState>) -> AppResult<ToolsStatus> {
    let emitter = app.clone();
    let manager = state.tool_manager.clone();
    let tools = state.tools.clone();
    let report = move |msg: String| {
        let _ = emitter.emit("tools://progress", msg);
    };
    let result = manager.install_missing(&report).await;
    let mut status = manager.status(tools.as_ref()).await;
    if let Err(e) = &result {
        status.message = Some(e.message.clone());
    }
    let _ = app.emit("tools://status", &status);
    result.map(|_| status)
}

/// Save custom tool paths and apply them immediately.
#[tauri::command]
#[specta::specta]
pub async fn set_tool_paths(
    state: State<'_, AppState>,
    paths: crate::commands::settings::ToolPaths,
) -> AppResult<ToolsStatus> {
    let p = paths.clone();
    state
        .db
        .call(move |c| {
            let s = crate::commands::settings::apply_patch(
                crate::commands::settings::load(c)?,
                crate::commands::settings::SettingsPatch {
                    tool_paths: Some(p),
                    ..Default::default()
                },
            )?;
            crate::commands::settings::save(c, &s)
        })
        .await?;
    state.tool_manager.locator().set_overrides(paths);
    Ok(state.tool_manager.status(state.tools.as_ref()).await)
}
