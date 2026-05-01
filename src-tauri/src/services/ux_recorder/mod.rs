mod session_store;
mod uxr_parser;

use serde::Serialize;
use serde_json::Value;
use tauri::{Emitter, Manager};
use tokio::time::{sleep, Duration};

use ms_manager_core::BridgeInstanceBinding;

use crate::api_error::ApiResult;
use crate::layout::PayloadLayout;
use crate::services::bridge_logs::BridgeLogEvent;
use crate::state::AppState;

pub use session_store::UxRecordingSessionInfo;

const UX_RECORDER_EVENT: &str = "ms-manager://ux-recorder";
const ENCODER_IDLE_FLUSH: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UxRecorderEvent {
    SessionStarted {
        instance_id: String,
        path: String,
        trigger: String,
    },
    EventRecorded {
        instance_id: String,
        path: String,
        event_count: u64,
        summary: String,
        presentation: uxr_parser::UxEventPresentation,
    },
    SessionEnded {
        instance_id: String,
        path: String,
        reason: String,
        event_count: u64,
        raw_event_count: u64,
    },
    Error {
        instance_id: Option<String>,
        message: String,
    },
}

pub fn observe_bridge_log(app: &tauri::AppHandle, event: &BridgeLogEvent) {
    let Some(payload) = uxr_parser::parse_uxr_payload(&event.message) else {
        return;
    };

    if let Err(error) = record_bridge_ux_event(app, event, payload) {
        let _ = app.emit(
            UX_RECORDER_EVENT,
            UxRecorderEvent::Error {
                instance_id: event.instance_id.clone(),
                message: error.message,
            },
        );
    }
}

pub fn close_session_for_instance(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    instance_id: &str,
    reason: &str,
) {
    let Some(session) = session_store::remove_session(instance_id) else {
        return;
    };
    if let Ok(closed) = session_store::close_session(layout, session, reason) {
        emit_session_ended(app, closed);
    }
}

pub fn close_all_sessions(app: &tauri::AppHandle, reason: &str) {
    let layout = app.state::<AppState>().layout_get();
    for session in session_store::drain_sessions() {
        if let Ok(closed) = session_store::close_session(&layout, session, reason) {
            emit_session_ended(app, closed);
        }
    }
}

pub fn open_recordings_folder(layout: &PayloadLayout) -> ApiResult<std::path::PathBuf> {
    session_store::open_recordings_folder(layout)
}

pub fn rotate_session(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    binding: &BridgeInstanceBinding,
) -> ApiResult<UxRecordingSessionInfo> {
    close_session_for_instance(app, layout, &binding.instance_id, "manual_rotate");
    let session = start_session_for_binding(app, layout, binding, "manual")?;
    Ok(session_store::session_info(&session))
}

fn record_bridge_ux_event(
    app: &tauri::AppHandle,
    event: &BridgeLogEvent,
    payload: Value,
) -> ApiResult<()> {
    let state = app.state::<AppState>();
    let layout = state.layout_get();
    let binding = binding_for_event(&state, event);
    let instance_id = binding
        .as_ref()
        .map(|binding| binding.instance_id.clone())
        .or_else(|| event.instance_id.clone())
        .unwrap_or_else(|| format!("port-{}", event.port));

    if uxr_parser::is_boot_marker(&payload) {
        close_session_for_instance(app, &layout, &instance_id, "new_boot");
        let session = match binding.as_ref() {
            Some(binding) => start_session_for_binding(app, &layout, binding, "boot")?,
            None => start_unbound_session(app, &layout, &instance_id, "boot")?,
        };
        write_ux_event(app, &layout, session, event, payload)?;
        return Ok(());
    }

    let session = match session_store::existing_session(&instance_id) {
        Some(session) => session,
        None => match binding.as_ref() {
            Some(binding) => start_session_for_binding(app, &layout, binding, "observed_event")?,
            None => start_unbound_session(app, &layout, &instance_id, "observed_event")?,
        },
    };

    write_ux_event(app, &layout, session, event, payload)
}

fn start_session_for_binding(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    binding: &BridgeInstanceBinding,
    trigger: &str,
) -> ApiResult<session_store::ActiveSession> {
    let session = session_store::start_session(
        layout,
        &binding.instance_id,
        Some(&binding.controller_serial),
        trigger,
    )?;
    emit_session_started(app, &session, trigger);
    Ok(session)
}

fn start_unbound_session(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    instance_id: &str,
    trigger: &str,
) -> ApiResult<session_store::ActiveSession> {
    let session = session_store::start_session(layout, instance_id, None, trigger)?;
    emit_session_started(app, &session, trigger);
    Ok(session)
}

fn write_ux_event(
    app: &tauri::AppHandle,
    layout: &PayloadLayout,
    session: session_store::ActiveSession,
    event: &BridgeLogEvent,
    payload: Value,
) -> ApiResult<()> {
    let outcome = session_store::write_event(layout, session, event, payload)?;
    handle_write_outcome(app, outcome);
    Ok(())
}

fn handle_write_outcome(app: &tauri::AppHandle, outcome: session_store::WriteEventOutcome) {
    let events = match outcome {
        session_store::WriteEventOutcome::Pending { instance_id } => {
            schedule_pending_encoder_flush(app, instance_id);
            return;
        }
        session_store::WriteEventOutcome::Recorded {
            events,
            pending_instance_id,
        } => {
            if let Some(instance_id) = pending_instance_id {
                schedule_pending_encoder_flush(app, instance_id);
            }
            events
        }
    };
    emit_recorded_events(app, events);
}

fn emit_recorded_events(app: &tauri::AppHandle, events: Vec<session_store::RecordedEvent>) {
    for recorded in events {
        let summary = uxr_parser::summarize_ux_event(&recorded.line);
        let presentation = uxr_parser::present_ux_event(&recorded.line);
        let _ = app.emit(
            UX_RECORDER_EVENT,
            UxRecorderEvent::EventRecorded {
                instance_id: recorded.instance_id,
                path: recorded.path.display().to_string(),
                event_count: recorded.event_count,
                summary,
                presentation,
            },
        );
    }
}

fn schedule_pending_encoder_flush(app: &tauri::AppHandle, instance_id: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            sleep(ENCODER_IDLE_FLUSH).await;
            let layout = app.state::<AppState>().layout_get();
            match session_store::flush_pending_encoder_turn_if_idle(
                &layout,
                &instance_id,
                ENCODER_IDLE_FLUSH,
            ) {
                Ok(session_store::WriteEventOutcome::Pending { .. }) => continue,
                Ok(outcome) => {
                    handle_write_outcome(&app, outcome);
                    break;
                }
                Err(error) => {
                    let _ = app.emit(
                        UX_RECORDER_EVENT,
                        UxRecorderEvent::Error {
                            instance_id: Some(instance_id),
                            message: error.message,
                        },
                    );
                    break;
                }
            }
        }
    });
}

fn binding_for_event(state: &AppState, event: &BridgeLogEvent) -> Option<BridgeInstanceBinding> {
    let instances = state.bridge_instances_get().instances;
    if let Some(instance_id) = event.instance_id.as_deref() {
        if let Some(binding) = instances
            .iter()
            .find(|binding| binding.instance_id == instance_id)
            .cloned()
        {
            return Some(binding);
        }
    }

    instances
        .into_iter()
        .find(|binding| binding.log_broadcast_port == event.port)
}

fn emit_session_started(
    app: &tauri::AppHandle,
    session: &session_store::ActiveSession,
    trigger: &str,
) {
    let _ = app.emit(
        UX_RECORDER_EVENT,
        UxRecorderEvent::SessionStarted {
            instance_id: session.instance_id.clone(),
            path: session.path.display().to_string(),
            trigger: trigger.to_string(),
        },
    );
}

fn emit_session_ended(app: &tauri::AppHandle, closed: session_store::ClosedSession) {
    let _ = app.emit(
        UX_RECORDER_EVENT,
        UxRecorderEvent::SessionEnded {
            instance_id: closed.instance_id,
            path: closed.path.display().to_string(),
            reason: closed.reason,
            event_count: closed.event_count,
            raw_event_count: closed.raw_event_count,
        },
    );
}
