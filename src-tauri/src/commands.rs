//! Tauri command handlers for Lux launcher.
//!
//! These commands are the interface between the TypeScript frontend
//! and the Rust backend.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::lua_runtime::LuaRuntime;
use crate::plugin_api::{Groups, Item, QueryEngine, ViewState};

/// Search for items matching the query.
///
/// Returns grouped results from triggers and sources.
#[tauri::command]
pub async fn search(
    query: String,
    engine: State<'_, Arc<QueryEngine>>,
    lua_runtime: State<'_, Option<Arc<LuaRuntime>>>,
) -> Result<Groups, String> {
    let rt = lua_runtime
        .as_ref()
        .ok_or_else(|| "No Lua runtime available".to_string())?;

    let engine = Arc::clone(&*engine);
    let query = query.clone();

    rt.with_lua(move |lua| engine.search(lua, &query)).await
}

/// Action info DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionInfoDto {
    pub plugin_name: String,
    pub action_index: usize,
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub bulk: bool,
}

/// Get applicable actions for items.
#[tauri::command]
pub async fn get_actions(
    items: Vec<Item>,
    engine: State<'_, Arc<QueryEngine>>,
    lua_runtime: State<'_, Option<Arc<LuaRuntime>>>,
) -> Result<Vec<ActionInfoDto>, String> {
    let rt = lua_runtime
        .as_ref()
        .ok_or_else(|| "No Lua runtime available".to_string())?;

    let engine = Arc::clone(&*engine);

    let actions = rt
        .with_lua(move |lua| engine.get_applicable_actions(lua, &items))
        .await?;

    Ok(actions
        .into_iter()
        .map(|a| ActionInfoDto {
            plugin_name: a.plugin_name,
            action_index: a.action_index,
            id: a.id,
            title: a.title,
            icon: a.icon,
            bulk: a.bulk,
        })
        .collect())
}

/// Action result DTO for frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActionResultDto {
    Dismiss,
    Pushed,
    Replaced,
    Popped,
    Progress { message: String },
    Complete { message: String },
    Fail { error: String },
    None,
}

impl From<crate::plugin_api::ActionResult> for ActionResultDto {
    fn from(result: crate::plugin_api::ActionResult) -> Self {
        use crate::plugin_api::ActionResult;
        match result {
            ActionResult::Dismiss => ActionResultDto::Dismiss,
            ActionResult::PushView { .. } => ActionResultDto::Pushed,
            ActionResult::ReplaceView { .. } => ActionResultDto::Replaced,
            ActionResult::Pop => ActionResultDto::Popped,
            ActionResult::Progress { message } => ActionResultDto::Progress { message },
            ActionResult::Complete { message, .. } => ActionResultDto::Complete { message },
            ActionResult::Fail { error } => ActionResultDto::Fail { error },
            ActionResult::Continue => ActionResultDto::None,
        }
    }
}

/// Execute an action on items.
#[tauri::command]
pub async fn execute_action(
    plugin_name: String,
    action_index: usize,
    items: Vec<Item>,
    engine: State<'_, Arc<QueryEngine>>,
    lua_runtime: State<'_, Option<Arc<LuaRuntime>>>,
) -> Result<ActionResultDto, String> {
    let rt = lua_runtime
        .as_ref()
        .ok_or_else(|| "No Lua runtime available".to_string())?;

    let engine = Arc::clone(&*engine);

    let result = rt
        .with_lua(move |lua| engine.execute_action(lua, &plugin_name, action_index, &items))
        .await?;

    Ok(ActionResultDto::from(result))
}

/// Execute the default action for items.
#[tauri::command]
pub async fn execute_default_action(
    items: Vec<Item>,
    engine: State<'_, Arc<QueryEngine>>,
    lua_runtime: State<'_, Option<Arc<LuaRuntime>>>,
) -> Result<ActionResultDto, String> {
    if items.is_empty() {
        return Ok(ActionResultDto::None);
    }

    let rt = lua_runtime
        .as_ref()
        .ok_or_else(|| "No Lua runtime available".to_string())?;

    let engine_arc = Arc::clone(&*engine);
    let items_for_default = items.clone();

    // Get the default action
    let action = rt
        .with_lua(move |lua| engine_arc.get_default_action(lua, &items_for_default))
        .await?;

    match action {
        Some(action_info) => {
            let engine_arc = Arc::clone(&*engine);
            let result = rt
                .with_lua(move |lua| {
                    engine_arc.execute_action(
                        lua,
                        &action_info.plugin_name,
                        action_info.action_index,
                        &items,
                    )
                })
                .await?;

            Ok(ActionResultDto::from(result))
        }
        None => Ok(ActionResultDto::None),
    }
}

/// Pop the current view from the stack.
#[tauri::command]
pub async fn pop_view(engine: State<'_, Arc<QueryEngine>>) -> Result<bool, String> {
    Ok(engine.pop_view())
}

/// Pop to a specific view in the stack.
#[tauri::command]
pub async fn pop_to_view(index: usize, engine: State<'_, Arc<QueryEngine>>) -> Result<(), String> {
    let current_len = engine.get_view_stack().len();
    if index >= current_len {
        return Err("Invalid view index".to_string());
    }

    for _ in 0..(current_len - index - 1) {
        engine.pop_view();
    }

    Ok(())
}

/// Get current view state.
#[tauri::command]
pub async fn get_view_state(
    engine: State<'_, Arc<QueryEngine>>,
) -> Result<Option<ViewState>, String> {
    Ok(engine.get_current_view_state())
}

/// Get the view stack for breadcrumbs.
#[tauri::command]
pub async fn get_view_stack(engine: State<'_, Arc<QueryEngine>>) -> Result<Vec<ViewState>, String> {
    Ok(engine.get_view_stack())
}
