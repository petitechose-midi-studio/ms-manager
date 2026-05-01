use serde::Serialize;
use serde_json::{Map, Value};

const UXR_PREFIX: &str = "UXR ";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct UxEventPresentation {
    pub kind: String,
    pub action: String,
    pub control: Option<String>,
    pub value: Option<String>,
    pub target: Option<String>,
    pub effect: Option<String>,
    pub state: Option<String>,
    pub detail: Option<String>,
}

pub(super) fn parse_uxr_payload(message: &str) -> Option<Value> {
    let trimmed = message.trim();
    let payload = trimmed
        .strip_prefix(UXR_PREFIX)
        .or_else(|| trimmed.split_once(UXR_PREFIX).map(|(_, payload)| payload))?;
    serde_json::from_str(payload.trim()).ok()
}

pub(super) fn is_boot_marker(payload: &Value) -> bool {
    let Some(obj) = payload.as_object() else {
        return false;
    };
    obj.get("kind").and_then(Value::as_str) == Some("session")
        && obj.get("event").and_then(Value::as_str) == Some("boot")
        && obj.get("enabled").and_then(Value::as_i64).unwrap_or(0) != 0
}

pub(super) fn summarize_ux_event(value: &Value) -> String {
    let presentation = present_ux_event(value);
    [
        Some(presentation.kind),
        Some(presentation.action),
        presentation.control,
        presentation.value,
        presentation.target,
        presentation.effect,
        presentation.state,
        presentation.detail,
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

pub(super) fn present_ux_event(value: &Value) -> UxEventPresentation {
    let Some(obj) = value.as_object() else {
        return fallback_presentation("event", "recorded");
    };
    match obj.get("kind").and_then(Value::as_str) {
        Some("button") => {
            let gesture = obj
                .get("gesture")
                .and_then(Value::as_str)
                .unwrap_or("event");
            let button = obj
                .get("button")
                .and_then(Value::as_str)
                .unwrap_or("button");
            UxEventPresentation {
                kind: "button".to_string(),
                action: gesture.to_string(),
                control: Some(button.to_string()),
                value: state_delta(obj),
                target: target_label(obj),
                effect: effect_label(obj),
                state: state_transition(obj),
                detail: None,
            }
        }
        Some("encoder") => {
            let gesture = obj
                .get("gesture")
                .and_then(Value::as_str)
                .unwrap_or("event");
            let encoder = obj
                .get("encoder")
                .and_then(Value::as_str)
                .unwrap_or("encoder");
            let count = obj.get("count").and_then(Value::as_u64);
            let value = match obj.get("value_kind").and_then(Value::as_str) {
                Some("delta") => obj
                    .get("delta_milli")
                    .and_then(Value::as_i64)
                    .map(format_delta_milli),
                Some("absolute") => format_absolute_value(obj),
                _ => None,
            };
            let detail = encoder_detail(count, obj.get("duration_ms").and_then(Value::as_i64));
            UxEventPresentation {
                kind: "encoder".to_string(),
                action: gesture.to_string(),
                control: Some(encoder.to_string()),
                value,
                target: target_label(obj),
                effect: effect_label(obj),
                state: state_transition(obj).or_else(|| current_state(obj)),
                detail,
            }
        }
        Some("session") => {
            let event = obj.get("event").and_then(Value::as_str).unwrap_or("event");
            fallback_presentation("session", event)
        }
        Some(kind) => fallback_presentation(kind, "recorded"),
        None => fallback_presentation("event", "recorded"),
    }
}

fn fallback_presentation(kind: &str, action: &str) -> UxEventPresentation {
    UxEventPresentation {
        kind: kind.to_string(),
        action: action.to_string(),
        control: None,
        value: None,
        target: None,
        effect: None,
        state: None,
        detail: None,
    }
}

fn target_label(obj: &Map<String, Value>) -> Option<String> {
    match obj.get("target").and_then(Value::as_str)? {
        "macro" => {
            let index = obj.get("target_index").and_then(Value::as_i64);
            let property = obj.get("property").and_then(Value::as_str);
            Some(match (index, property) {
                (Some(index), Some(property)) => format!("macro {} {property}", index + 1),
                (Some(index), None) => format!("macro {}", index + 1),
                (None, Some(property)) => format!("macro {property}"),
                (None, None) => "macro".to_string(),
            })
        }
        "view" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|view| format!("view {view}"))
            .or_else(|| Some("view".to_string())),
        "quick_control" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("quick {property}"))
            .or_else(|| Some("quick control".to_string())),
        "macro_config" => {
            let index = obj.get("target_index").and_then(Value::as_i64);
            let property = obj.get("property").and_then(Value::as_str);
            Some(match (index, property) {
                (Some(index), Some(property)) => format!("macro {} {property}", index + 1),
                (Some(index), None) => format!("macro {}", index + 1),
                (None, Some(property)) => format!("macro {property}"),
                (None, None) => "macro config".to_string(),
            })
        }
        "macro_config_value" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("macro value {property}"))
            .or_else(|| Some("macro value".to_string())),
        "macro_property" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("macro {property}"))
            .or_else(|| Some("macro property".to_string())),
        "page" | "track" => {
            let target = obj.get("target").and_then(Value::as_str).unwrap_or("item");
            let index = display_index_or_range(obj, "target_index");
            let property = obj.get("property").and_then(Value::as_str);
            Some(match (index, property) {
                (Some(index), Some("add_slot")) => format!("{target} new {index}"),
                (Some(index), Some("selection")) => format!("{target} selection {index}"),
                (Some(index), _) => format!("{target} {index}"),
                (None, Some("add_slot")) => format!("{target} new"),
                (None, Some("selection")) => format!("{target} selection"),
                (None, _) => target.to_string(),
            })
        }
        "step" => {
            let step = obj.get("target_step").and_then(Value::as_i64)?;
            let property = obj.get("property").and_then(Value::as_str);
            let display_step = step + 1;
            Some(match property {
                Some(property) => format!("step {display_step} {property}"),
                None => format!("step {display_step}"),
            })
        }
        "property" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("property {property}"))
            .or_else(|| Some("property".to_string())),
        "setting" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("setting {property}"))
            .or_else(|| Some("setting".to_string())),
        "setting_value" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("value {property}"))
            .or_else(|| Some("setting value".to_string())),
        "manager" => Some("manager".to_string()),
        "shortcut" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("shortcut {property}"))
            .or_else(|| Some("shortcut".to_string())),
        "shortcut_command" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("shortcut command {property}"))
            .or_else(|| Some("shortcut command".to_string())),
        "command" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("command {property}"))
            .or_else(|| Some("command".to_string())),
        "slot" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("slot {property}"))
            .or_else(|| Some("slot".to_string())),
        "load_mode" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("load {property}"))
            .or_else(|| Some("load mode".to_string())),
        "confirmation" => obj
            .get("property")
            .and_then(Value::as_str)
            .map(|property| format!("confirm {property}"))
            .or_else(|| Some("confirm".to_string())),
        "settings" => Some("settings".to_string()),
        "transport" => Some("transport".to_string()),
        target => Some(target.to_string()),
    }
}

fn effect_label(obj: &Map<String, Value>) -> Option<String> {
    let label = match obj.get("effect").and_then(Value::as_str)? {
        "edit_step_property" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("edit={value}"))
            .or_else(|| Some("edit".to_string())),
        "toggle_step" => obj
            .get("step_on")
            .and_then(Value::as_i64)
            .map(|value| if value != 0 { "on" } else { "off" }.to_string())
            .or_else(|| Some("toggle".to_string())),
        "select_property" => property_transition(obj).or_else(|| Some("select".to_string())),
        "apply_property" => Some("apply".to_string()),
        "cancel_property" => Some("cancel".to_string()),
        "open_global_settings" => Some("open".to_string()),
        "close_global_settings" => Some("close".to_string()),
        "focus_setting" => Some("focus".to_string()),
        "open_setting_value" => Some("open value".to_string()),
        "select_setting_value" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("select={value}"))
            .or_else(|| Some("select".to_string())),
        "apply_setting_value" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("apply={value}"))
            .or_else(|| Some("apply".to_string())),
        "cancel_setting_value" => Some("cancel".to_string()),
        "transport_start" => Some("start".to_string()),
        "transport_stop" => Some("stop".to_string()),
        "open_view_selector" => Some("open".to_string()),
        "select_view" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("select={value}"))
            .or_else(|| Some("select".to_string())),
        "apply_view" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("apply={value}"))
            .or_else(|| Some("apply".to_string())),
        "edit_macro_value" | "edit_macro_cc" | "edit_macro_channel" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("edit={value}"))
            .or_else(|| Some("edit".to_string())),
        "open_macro_edit" => Some("open".to_string()),
        "focus_macro_config" => Some("focus".to_string()),
        "edit_macro_config" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("edit={value}"))
            .or_else(|| Some("edit".to_string())),
        "open_macro_config_value" => Some("open value".to_string()),
        "close_macro_edit" => Some("close".to_string()),
        "open_macro_page_selector" => Some("open page".to_string()),
        "open_macro_target_selector" => Some("open target".to_string()),
        "select_macro_config_value" | "select_macro_page" | "select_macro_target" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("select={value}"))
            .or_else(|| Some("select".to_string())),
        "apply_macro_config_value" | "apply_macro_page" | "apply_macro_target" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("apply={value}"))
            .or_else(|| Some("apply".to_string())),
        "cancel_macro_config_value" | "cancel_macro_page" | "cancel_macro_target" => {
            Some("cancel".to_string())
        }
        "apply_macro_edit" => Some("apply".to_string()),
        "open_macro_clutch" => Some("open".to_string()),
        "select_macro_property" => property_transition(obj).or_else(|| Some("select".to_string())),
        "apply_macro_clutch" => Some("apply".to_string()),
        "open_quick_controls" => Some("open".to_string()),
        "select_quick_control" => property_transition(obj).or_else(|| Some("select".to_string())),
        "edit_quick_control" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("edit={value}"))
            .or_else(|| Some("edit".to_string())),
        "apply_quick_controls" => Some("apply".to_string()),
        "cancel_quick_controls" => Some("cancel".to_string()),
        "open_data_manager" => Some("open".to_string()),
        "focus_data_manager_shortcut" => Some("focus".to_string()),
        "open_shortcut_assignment" => Some("assign".to_string()),
        "run_left_shortcut" => Some("run left".to_string()),
        "open_command_palette" => Some("commands".to_string()),
        "run_right_shortcut" => Some("run right".to_string()),
        "close_data_manager" => Some("close".to_string()),
        "select_data_manager_item" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("select={value}"))
            .or_else(|| Some("select".to_string())),
        "apply_data_manager_item" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("apply={value}"))
            .or_else(|| Some("apply".to_string())),
        "cancel_data_manager_dialog" => Some("cancel".to_string()),
        "open_property_selector" => Some("open".to_string()),
        "open_step_edit" => Some("open".to_string()),
        "focus_step_edit_property" => {
            property_transition(obj).or_else(|| Some("focus".to_string()))
        }
        "edit_step_edit_property" => obj
            .get("value_label")
            .and_then(Value::as_str)
            .map(|value| format!("edit={value}"))
            .or_else(|| Some("edit".to_string())),
        "apply_step_edit" => Some("apply".to_string()),
        "cancel_step_edit" => Some("cancel".to_string()),
        "preview_structure" => Some("preview".to_string()),
        "enter_selection" => Some("select mode".to_string()),
        "switch_structure_focus" => Some("focus".to_string()),
        "create_structure" => Some("create".to_string()),
        "arm_remove" => Some("hold remove".to_string()),
        "erase_structure" => Some("erase".to_string()),
        "remove_structure" => Some("remove".to_string()),
        "arm_paste" => Some("hold paste".to_string()),
        "copy_structure" => Some("copy".to_string()),
        "paste_structure" => Some("paste".to_string()),
        "navigate_selection" => Some("navigate".to_string()),
        "toggle_selection" => Some("toggle".to_string()),
        "cancel_selection" => Some("cancel".to_string()),
        "delete_selection" => Some("delete".to_string()),
        "duplicate_selection" => Some("duplicate".to_string()),
        "release_ignored" => Some("ignored".to_string()),
        effect => Some(effect.to_string()),
    }?;
    Some(apply_outcome_label(obj, label))
}

fn apply_outcome_label(obj: &Map<String, Value>, label: String) -> String {
    match obj.get("outcome").and_then(Value::as_str) {
        Some("ignored") => match obj.get("reason").and_then(Value::as_str) {
            Some("after_long_press") => "ignored after long press".to_string(),
            Some(reason) => format!("ignored {reason}"),
            None => "ignored".to_string(),
        },
        Some("noop") => match obj.get("reason").and_then(Value::as_str) {
            Some("add_slot") => format!("{label} noop add-slot"),
            Some("clipboard_empty") => format!("{label} noop clipboard-empty"),
            Some("single_slot") => format!("{label} noop single-slot"),
            Some(reason) => format!("{label} noop {reason}"),
            None => format!("{label} noop"),
        },
        _ => label,
    }
}

fn property_transition(obj: &Map<String, Value>) -> Option<String> {
    let pre = obj.get("pre_property").and_then(Value::as_str);
    let post = obj.get("property").and_then(Value::as_str);
    match (pre, post) {
        (Some(pre), Some(post)) if pre != post => Some(format!("{pre}->{post}")),
        (_, Some(post)) => Some(post.to_string()),
        _ => None,
    }
}

fn state_transition(obj: &Map<String, Value>) -> Option<String> {
    let pre_view = obj.get("pre_view").and_then(Value::as_str);
    let pre_overlay = obj.get("pre_overlay").and_then(Value::as_str);
    let view = obj.get("view").and_then(Value::as_str);
    let overlay = obj.get("overlay").and_then(Value::as_str);
    match (pre_view, pre_overlay, view, overlay) {
        (Some(pre_view), Some(pre_overlay), Some(view), Some(overlay))
            if pre_view != view || pre_overlay != overlay =>
        {
            Some(format!("{pre_view}/{pre_overlay}->{view}/{overlay}"))
        }
        _ => None,
    }
}

fn current_state(obj: &Map<String, Value>) -> Option<String> {
    let view = obj.get("view").and_then(Value::as_str)?;
    let overlay = obj.get("overlay").and_then(Value::as_str)?;
    Some(format!("{view}/{overlay}"))
}

fn state_delta(obj: &Map<String, Value>) -> Option<String> {
    let mut parts = Vec::new();
    push_i64_delta(&mut parts, "play", obj, "pre_playing", "playing");
    push_i64_delta(&mut parts, "head", obj, "pre_playhead", "playhead");
    push_index_delta(&mut parts, "page", obj, "pre_page", "page");
    push_index_delta(
        &mut parts,
        "shared",
        obj,
        "pre_shared_track",
        "shared_track",
    );
    push_i64_delta(&mut parts, "mask", obj, "pre_shared_mask", "shared_mask");
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn push_index_delta(
    parts: &mut Vec<String>,
    label: &str,
    obj: &Map<String, Value>,
    from: &str,
    to: &str,
) {
    let Some(from) = obj.get(from).and_then(Value::as_i64) else {
        return;
    };
    let Some(to) = obj.get(to).and_then(Value::as_i64) else {
        return;
    };
    if from != to {
        parts.push(format!("{label}={}->{}", from + 1, to + 1));
    }
}

fn display_index_or_range(obj: &Map<String, Value>, key: &str) -> Option<String> {
    if let Some(index) = obj.get(key).and_then(Value::as_i64) {
        return Some((index + 1).to_string());
    }

    let first = obj.get(&format!("first_{key}")).and_then(Value::as_i64)?;
    let last = obj.get(&format!("last_{key}")).and_then(Value::as_i64)?;
    if first == last {
        Some((first + 1).to_string())
    } else {
        Some(format!("{}->{}", first + 1, last + 1))
    }
}

fn push_i64_delta(
    parts: &mut Vec<String>,
    label: &str,
    obj: &Map<String, Value>,
    from: &str,
    to: &str,
) {
    let Some(from) = obj.get(from).and_then(Value::as_i64) else {
        return;
    };
    let Some(to) = obj.get(to).and_then(Value::as_i64) else {
        return;
    };
    if from != to {
        parts.push(format!("{label}={from}->{to}"));
    }
}

fn format_delta_milli(value: i64) -> String {
    let scaled = value as f64 / 1000.0;
    if scaled >= 0.0 {
        format!("Δ+{scaled:.1}")
    } else {
        format!("Δ{scaled:.1}")
    }
}

fn format_absolute_value(obj: &Map<String, Value>) -> Option<String> {
    let first = obj.get("first_value_milli").and_then(Value::as_i64);
    let last = obj
        .get("last_value_milli")
        .and_then(Value::as_i64)
        .or_else(|| obj.get("value_milli").and_then(Value::as_i64));
    match (first, last) {
        (Some(first), Some(last)) if first != last => Some(format!(
            "{}->{}",
            format_percent(first),
            format_percent(last)
        )),
        (_, Some(last)) => Some(format!("={}", format_percent(last))),
        _ => None,
    }
}

fn format_percent(value_milli: i64) -> String {
    format!("{:.1}%", value_milli as f64 / 10.0)
}

fn encoder_detail(count: Option<u64>, duration_ms: Option<i64>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(count) = count.filter(|count| *count > 1) {
        parts.push(format!("x{count}"));
    }
    if let Some(duration_ms) = duration_ms {
        parts.push(format!("{duration_ms}ms"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uxr_payload_ignores_regular_logs() {
        assert!(parse_uxr_payload("Ready").is_none());
        assert!(parse_uxr_payload("UXR {\"kind\":\"session\"}").is_some());
        assert!(parse_uxr_payload("[123ms] INFO: UXR {\"kind\":\"session\"}").is_some());
    }

    #[test]
    fn boot_marker_requires_enabled_session_boot() {
        assert!(is_boot_marker(&serde_json::json!({
            "kind": "session",
            "event": "boot",
            "enabled": 1
        })));
        assert!(!is_boot_marker(&serde_json::json!({
            "kind": "session",
            "event": "boot",
            "enabled": 0
        })));
    }

    #[test]
    fn summarize_button_event_includes_transition_when_available() {
        let value = serde_json::json!({
            "kind": "button",
            "gesture": "release",
            "button": "LEFT_TOP",
            "target": "step",
            "target_step": 3,
            "property": "Gate",
            "effect": "toggle_step",
            "step_on": 1,
            "pre_view": "macro",
            "pre_overlay": "view_selector",
            "view": "sequencer",
            "overlay": "none"
        });
        assert_eq!(
            summarize_ux_event(&value),
            "button release LEFT_TOP step 4 Gate on macro/view_selector->sequencer/none"
        );
    }

    #[test]
    fn summarize_encoder_event_distinguishes_delta_and_absolute_values() {
        let delta = serde_json::json!({
            "kind": "encoder",
            "gesture": "turn",
            "encoder": "NAV",
            "value_kind": "delta",
            "delta_milli": -1000,
            "view": "sequencer",
            "overlay": "none"
        });
        let absolute = serde_json::json!({
            "kind": "encoder",
            "gesture": "turn",
            "encoder": "MACRO_1",
            "value_kind": "absolute",
            "value_milli": 128,
            "first_value_milli": 199,
            "last_value_milli": 128,
            "target": "step",
            "target_step": 2,
            "property": "Velocity",
            "effect": "edit_step_property",
            "value_label": "128",
            "count": 2,
            "duration_ms": 50,
            "view": "macro",
            "overlay": "none"
        });

        assert_eq!(
            summarize_ux_event(&delta),
            "encoder turn NAV Δ-1.0 sequencer/none"
        );
        assert_eq!(
            summarize_ux_event(&absolute),
            "encoder turn MACRO_1 19.9%->12.8% step 3 Velocity edit=128 macro/none x2 50ms"
        );
    }

    #[test]
    fn summarize_state_indices_as_user_visible_numbers() {
        let value = serde_json::json!({
            "kind": "button",
            "gesture": "release",
            "button": "NAV",
            "target": "track",
            "target_index": 1,
            "effect": "create_structure",
            "pre_page": 2,
            "page": 0,
            "pre_shared_track": 0,
            "shared_track": 1,
            "pre_shared_mask": 1,
            "shared_mask": 3,
            "view": "sequencer",
            "overlay": "none"
        });

        assert_eq!(
            summarize_ux_event(&value),
            "button release NAV page=3->1 shared=1->2 mask=1->3 track 2 create"
        );
    }

    #[test]
    fn summarize_coalesced_structure_scan_indices_as_user_visible_ranges() {
        let value = serde_json::json!({
            "kind": "encoder",
            "gesture": "turn",
            "encoder": "NAV",
            "value_kind": "delta",
            "delta_milli": 14000,
            "target": "track",
            "property": "add_slot",
            "first_target_index": 1,
            "last_target_index": 15,
            "effect": "preview_structure",
            "count": 14,
            "duration_ms": 900,
            "view": "sequencer",
            "overlay": "none"
        });

        assert_eq!(
            summarize_ux_event(&value),
            "encoder turn NAV Δ+14.0 track new 2->16 preview sequencer/none x14 900ms"
        );
    }
}
