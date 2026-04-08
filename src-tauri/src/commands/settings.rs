use std::collections::HashSet;

use tauri::State;

use crate::api_error::ApiResult;
use crate::models::{TabOrderResponse, TabOrderSetRequest};
use crate::state::AppState;

#[tauri::command]
pub fn tab_order_set(
    state: State<'_, AppState>,
    request: TabOrderSetRequest,
) -> ApiResult<TabOrderResponse> {
    let bindings = state.bridge_instances_get();
    let known_ids = bindings
        .instances
        .iter()
        .map(|instance| instance.instance_id.as_str())
        .collect::<HashSet<_>>();

    let mut next = Vec::new();
    let mut seen = HashSet::new();

    for instance_id in request.instance_ids {
        let instance_id = instance_id.trim();
        if instance_id.is_empty() || !known_ids.contains(instance_id) || !seen.insert(instance_id.to_string()) {
            continue;
        }
        next.push(instance_id.to_string());
    }

    for instance in bindings.instances {
        if seen.insert(instance.instance_id.clone()) {
            next.push(instance.instance_id);
        }
    }

    let settings = state.settings_set_tab_order(next)?;
    Ok(TabOrderResponse {
        tab_order: settings.tab_order,
    })
}
