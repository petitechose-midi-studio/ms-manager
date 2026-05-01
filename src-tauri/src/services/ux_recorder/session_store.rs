use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::api_error::{ApiError, ApiResult};
use crate::layout::PayloadLayout;
use crate::services::bridge_logs::BridgeLogEvent;
use crate::storage::write_json_atomic;

const UX_RECORDINGS_SCHEMA: u32 = 5;
const MAX_INDEX_SESSIONS: usize = 100;
const MAX_COALESCED_ENCODER_TURNS: u64 = 32;
const MAX_COALESCE_DURATION_MS: i64 = 2_000;

#[derive(Debug, Clone, Serialize)]
pub struct UxRecordingSessionInfo {
    pub instance_id: String,
    pub path: String,
    pub started_at: String,
    pub event_count: u64,
    pub raw_event_count: u64,
}

#[derive(Debug, Clone)]
pub(super) struct ActiveSession {
    pub instance_id: String,
    pub path: PathBuf,
    pub started_at: String,
    pub event_count: u64,
    pub raw_event_count: u64,
    last_written_ms: Option<i64>,
    pending_encoder_turn: Option<PendingEncoderTurn>,
    pending_encoder_flush_active: bool,
}

#[derive(Debug, Clone)]
pub(super) struct RecordedEvent {
    pub instance_id: String,
    pub path: PathBuf,
    pub event_count: u64,
    pub line: Value,
}

#[derive(Debug, Clone)]
pub(super) enum WriteEventOutcome {
    Pending {
        instance_id: String,
    },
    Recorded {
        events: Vec<RecordedEvent>,
        pending_instance_id: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub(super) struct ClosedSession {
    pub instance_id: String,
    pub path: PathBuf,
    pub reason: String,
    pub event_count: u64,
    pub raw_event_count: u64,
}

#[derive(Default)]
struct UxRecorderState {
    sessions: HashMap<String, ActiveSession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncoderTurnKey {
    encoder: String,
    value_kind: EncoderValueKind,
    mode: Option<String>,
    effect: Option<String>,
    target: Option<String>,
    target_index: Option<i64>,
    target_step: Option<i64>,
    pre_view: Option<String>,
    pre_overlay: Option<String>,
    view: Option<String>,
    overlay: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncoderValueKind {
    Delta,
    Absolute,
}

#[derive(Debug, Clone)]
struct PendingEncoderTurn {
    key: EncoderTurnKey,
    line: Value,
    value_kind: EncoderValueKind,
    coalesced_count: u64,
    total_delta_milli: i64,
    first_value_milli: Option<i64>,
    last_value_milli: Option<i64>,
    first_seq: Option<i64>,
    last_seq: Option<i64>,
    first_ms: Option<i64>,
    last_ms: Option<i64>,
    first_playhead: Option<i64>,
    last_playhead: Option<i64>,
    first_page: Option<i64>,
    last_page: Option<i64>,
    first_target_index: Option<i64>,
    last_target_index: Option<i64>,
    updated_at: Instant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UxRecordingIndex {
    schema: u32,
    latest: Option<String>,
    sessions: Vec<UxRecordingIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UxRecordingIndexEntry {
    path: String,
    instance_id: String,
    started_at: String,
    ended_at: Option<String>,
    trigger: String,
    end_reason: Option<String>,
    event_count: u64,
    raw_event_count: u64,
}

impl Default for UxRecordingIndex {
    fn default() -> Self {
        Self {
            schema: UX_RECORDINGS_SCHEMA,
            latest: None,
            sessions: Vec::new(),
        }
    }
}

static UX_RECORDER_STATE: OnceLock<Mutex<UxRecorderState>> = OnceLock::new();

pub(super) fn open_recordings_folder(layout: &PayloadLayout) -> ApiResult<PathBuf> {
    let dir = layout.ux_recordings_dir();
    std::fs::create_dir_all(&dir).map_err(|e| {
        ApiError::new(
            "io_create_dir_failed",
            format!("create UX recordings dir: {e}"),
        )
    })?;
    Ok(dir)
}

pub(super) fn existing_session(instance_id: &str) -> Option<ActiveSession> {
    state_mutex().sessions.get(instance_id).cloned()
}

pub(super) fn remove_session(instance_id: &str) -> Option<ActiveSession> {
    state_mutex().sessions.remove(instance_id)
}

pub(super) fn drain_sessions() -> Vec<ActiveSession> {
    state_mutex()
        .sessions
        .drain()
        .map(|(_, session)| session)
        .collect()
}

pub(super) fn start_session(
    layout: &PayloadLayout,
    instance_id: &str,
    controller_serial: Option<&str>,
    trigger: &str,
) -> ApiResult<ActiveSession> {
    let started_at = now_rfc3339();
    let dir = layout
        .ux_recordings_dir()
        .join(sanitize_path_segment(instance_id));
    std::fs::create_dir_all(&dir).map_err(|e| {
        ApiError::new(
            "io_create_dir_failed",
            format!("create UX session dir: {e}"),
        )
    })?;

    let path = unique_session_path(&dir, instance_id);
    let start_line = serde_json::json!({
        "type": "session_start",
        "schema": UX_RECORDINGS_SCHEMA,
        "started_at": started_at,
        "instance_id": instance_id,
        "controller_serial": controller_serial,
        "source": "ms-manager",
        "trigger": trigger,
    });
    append_json_line(&path, &start_line)?;

    let session = ActiveSession {
        instance_id: instance_id.to_string(),
        path,
        started_at,
        event_count: 0,
        raw_event_count: 0,
        last_written_ms: None,
        pending_encoder_turn: None,
        pending_encoder_flush_active: false,
    };

    state_mutex()
        .sessions
        .insert(instance_id.to_string(), session.clone());
    update_index_session_started(layout, &session, trigger)?;
    Ok(session)
}

pub(super) fn write_event(
    layout: &PayloadLayout,
    session: ActiveSession,
    event: &BridgeLogEvent,
    payload: Value,
) -> ApiResult<WriteEventOutcome> {
    let line = event_line(&session, event, payload);
    let pending = pending_encoder_turn_from_line(&line, event);

    let mut guard = state_mutex();
    let Some(active) = guard.sessions.get_mut(&session.instance_id) else {
        let mut session = session;
        let line = finalize_event_line(&mut session, line);
        append_json_line(&session.path, &line)?;
        return Ok(WriteEventOutcome::Recorded {
            events: vec![RecordedEvent {
                instance_id: session.instance_id,
                path: session.path,
                event_count: session.event_count + 1,
                line,
            }],
            pending_instance_id: None,
        });
    };
    active.raw_event_count += 1;

    if let Some(next_pending) = pending {
        if let Some(existing) = active.pending_encoder_turn.as_mut() {
            if existing.key == next_pending.key {
                existing.absorb(next_pending);
                let event_count = active.event_count;
                let raw_event_count = active.raw_event_count;
                let path = active.path.clone();
                if should_flush_encoder_turn(existing) {
                    let flushed = flush_pending_encoder_turn(active)?;
                    let event_count = active.event_count;
                    drop(guard);
                    update_index_counts(layout, &path, event_count, raw_event_count)?;
                    return Ok(recorded_outcome(flushed));
                }
                let schedule_flush = !active.pending_encoder_flush_active;
                active.pending_encoder_flush_active = true;
                drop(guard);
                update_index_counts(layout, &path, event_count, raw_event_count)?;
                return Ok(if schedule_flush {
                    pending_outcome_for(&session.instance_id)
                } else {
                    recorded_outcome(Vec::new())
                });
            }
        }

        let flushed = flush_pending_encoder_turn(active)?;
        active.pending_encoder_turn = Some(next_pending);
        let schedule_flush = !active.pending_encoder_flush_active;
        active.pending_encoder_flush_active = true;
        let event_count = active.event_count;
        let raw_event_count = active.raw_event_count;
        let path = active.path.clone();
        let instance_id = active.instance_id.clone();
        drop(guard);
        update_index_counts(layout, &path, event_count, raw_event_count)?;
        return Ok(recorded_or_pending_outcome(
            flushed,
            schedule_flush.then_some(instance_id),
        ));
    }

    let mut recorded = flush_pending_encoder_turn(active)?;
    let line = finalize_event_line(active, line);
    append_json_line(&active.path, &line)?;
    active.event_count += 1;
    let event_count = active.event_count;
    let raw_event_count = active.raw_event_count;
    recorded.push(RecordedEvent {
        instance_id: active.instance_id.clone(),
        path: active.path.clone(),
        event_count,
        line,
    });
    let path = active.path.clone();
    drop(guard);

    update_index_counts(layout, &path, event_count, raw_event_count)?;
    Ok(WriteEventOutcome::Recorded {
        events: recorded,
        pending_instance_id: None,
    })
}

pub(super) fn flush_pending_encoder_turn_if_idle(
    layout: &PayloadLayout,
    instance_id: &str,
    idle_for: Duration,
) -> ApiResult<WriteEventOutcome> {
    let mut guard = state_mutex();
    let Some(active) = guard.sessions.get_mut(instance_id) else {
        return Ok(recorded_outcome(Vec::new()));
    };
    let Some(pending) = active.pending_encoder_turn.as_ref() else {
        active.pending_encoder_flush_active = false;
        return Ok(recorded_outcome(Vec::new()));
    };
    if pending.updated_at.elapsed() < idle_for {
        return Ok(WriteEventOutcome::Pending {
            instance_id: instance_id.to_string(),
        });
    }

    let recorded = flush_pending_encoder_turn(active)?;
    let event_count = active.event_count;
    let raw_event_count = active.raw_event_count;
    let path = active.path.clone();
    drop(guard);

    update_index_counts(layout, &path, event_count, raw_event_count)?;
    Ok(recorded_outcome(recorded))
}

pub(super) fn close_session(
    layout: &PayloadLayout,
    session: ActiveSession,
    reason: &str,
) -> ApiResult<ClosedSession> {
    let mut session = session;
    flush_pending_encoder_turn(&mut session)?;
    let ended_at = now_rfc3339();
    let end_line = serde_json::json!({
        "type": "session_end",
        "ended_at": ended_at,
        "reason": reason,
        "event_count": session.event_count,
        "raw_event_count": session.raw_event_count,
    });
    append_json_line(&session.path, &end_line)?;
    update_index_session_ended(
        layout,
        &session.path,
        &ended_at,
        reason,
        session.event_count,
        session.raw_event_count,
    )?;

    Ok(ClosedSession {
        instance_id: session.instance_id,
        path: session.path,
        reason: reason.to_string(),
        event_count: session.event_count,
        raw_event_count: session.raw_event_count,
    })
}

pub(super) fn session_info(session: &ActiveSession) -> UxRecordingSessionInfo {
    UxRecordingSessionInfo {
        instance_id: session.instance_id.clone(),
        path: session.path.display().to_string(),
        started_at: session.started_at.clone(),
        event_count: session.event_count,
        raw_event_count: session.raw_event_count,
    }
}

fn event_line(_session: &ActiveSession, _event: &BridgeLogEvent, payload: Value) -> Value {
    let mut line = Map::new();
    line.insert("type".to_string(), Value::String("ux_event".to_string()));

    if let Value::Object(payload) = payload {
        copy_payload_fields(&mut line, &payload);
    } else {
        line.insert("payload".to_string(), payload);
    }

    Value::Object(line)
}

fn copy_payload_fields(line: &mut Map<String, Value>, payload: &Map<String, Value>) {
    for key in [
        "seq",
        "ms",
        "kind",
        "gesture",
        "button",
        "button_id",
        "encoder",
        "encoder_id",
        "value_kind",
        "delta_milli",
        "value_milli",
        "binding",
        "mode",
        "effect",
        "outcome",
        "reason",
        "target",
        "target_index",
        "target_step",
        "pre_target_mask",
        "target_mask",
        "pre_property",
        "property",
        "value_label",
        "step_on",
        "pre_view",
        "pre_overlay",
        "pre_playing",
        "pre_playhead",
        "pre_page",
        "pre_shared_track",
        "pre_shared_mask",
        "view",
        "overlay",
        "playing",
        "playhead",
        "page",
        "shared_track",
        "shared_mask",
    ] {
        if let Some(value) = payload.get(key) {
            line.insert(key.to_string(), value.clone());
        }
    }
}

fn recorded_outcome(recorded: Vec<RecordedEvent>) -> WriteEventOutcome {
    WriteEventOutcome::Recorded {
        events: recorded,
        pending_instance_id: None,
    }
}

fn pending_outcome_for(instance_id: &str) -> WriteEventOutcome {
    WriteEventOutcome::Pending {
        instance_id: instance_id.to_string(),
    }
}

fn recorded_or_pending_outcome(
    recorded: Vec<RecordedEvent>,
    pending_instance_id: Option<String>,
) -> WriteEventOutcome {
    if recorded.is_empty() {
        return match pending_instance_id {
            Some(instance_id) => WriteEventOutcome::Pending { instance_id },
            None => recorded_outcome(recorded),
        };
    }
    WriteEventOutcome::Recorded {
        events: recorded,
        pending_instance_id,
    }
}

fn should_flush_encoder_turn(pending: &PendingEncoderTurn) -> bool {
    pending.coalesced_count >= MAX_COALESCED_ENCODER_TURNS
        || pending
            .duration_ms()
            .is_some_and(|duration| duration >= MAX_COALESCE_DURATION_MS)
}

fn flush_pending_encoder_turn(session: &mut ActiveSession) -> ApiResult<Vec<RecordedEvent>> {
    let Some(pending) = session.pending_encoder_turn.take() else {
        return Ok(Vec::new());
    };

    let line = finalize_event_line(session, pending.into_line());
    append_json_line(&session.path, &line)?;
    session.event_count += 1;
    session.pending_encoder_flush_active = false;
    Ok(vec![RecordedEvent {
        instance_id: session.instance_id.clone(),
        path: session.path.clone(),
        event_count: session.event_count,
        line,
    }])
}

fn pending_encoder_turn_from_line(
    line: &Value,
    _event: &BridgeLogEvent,
) -> Option<PendingEncoderTurn> {
    let obj = line.as_object()?;
    if obj.get("kind").and_then(Value::as_str) != Some("encoder") {
        return None;
    }
    if obj.get("gesture").and_then(Value::as_str) != Some("turn") {
        return None;
    }

    let encoder = obj.get("encoder").and_then(Value::as_str)?.to_string();
    let value_kind = EncoderValueKind::from_object(obj)?;
    let value_milli = value_kind.value_milli(obj)?;
    let coalesce_target_motion =
        obj.get("effect").and_then(Value::as_str) == Some("preview_structure");
    let key = EncoderTurnKey {
        encoder,
        value_kind,
        mode: optional_string(obj, "mode"),
        effect: optional_string(obj, "effect"),
        target: optional_string(obj, "target"),
        target_index: if coalesce_target_motion {
            None
        } else {
            obj.get("target_index").and_then(Value::as_i64)
        },
        target_step: obj.get("target_step").and_then(Value::as_i64),
        pre_view: optional_string(obj, "pre_view"),
        pre_overlay: optional_string(obj, "pre_overlay"),
        view: optional_string(obj, "view"),
        overlay: optional_string(obj, "overlay"),
    };

    Some(PendingEncoderTurn {
        key,
        line: line.clone(),
        value_kind,
        coalesced_count: 1,
        total_delta_milli: if value_kind == EncoderValueKind::Delta {
            value_milli
        } else {
            0
        },
        first_value_milli: Some(value_milli),
        last_value_milli: Some(value_milli),
        first_seq: obj.get("seq").and_then(Value::as_i64),
        last_seq: obj.get("seq").and_then(Value::as_i64),
        first_ms: obj.get("ms").and_then(Value::as_i64),
        last_ms: obj.get("ms").and_then(Value::as_i64),
        first_playhead: obj.get("pre_playhead").and_then(Value::as_i64),
        last_playhead: obj.get("playhead").and_then(Value::as_i64),
        first_page: obj.get("pre_page").and_then(Value::as_i64),
        last_page: obj.get("page").and_then(Value::as_i64),
        first_target_index: obj.get("target_index").and_then(Value::as_i64),
        last_target_index: obj.get("target_index").and_then(Value::as_i64),
        updated_at: Instant::now(),
    })
}

fn optional_string(obj: &Map<String, Value>, key: &str) -> Option<String> {
    obj.get(key).and_then(Value::as_str).map(ToOwned::to_owned)
}

fn insert_range_if_changed(
    obj: &mut Map<String, Value>,
    name: &str,
    first: Option<i64>,
    last: Option<i64>,
) {
    if first == last {
        return;
    }
    if let Some(first) = first {
        obj.insert(format!("first_{name}"), Value::from(first));
    }
    if let Some(last) = last {
        obj.insert(format!("last_{name}"), Value::from(last));
    }
}

impl PendingEncoderTurn {
    fn absorb(&mut self, next: PendingEncoderTurn) {
        self.coalesced_count += 1;
        if self.value_kind == EncoderValueKind::Delta {
            self.total_delta_milli += next.total_delta_milli;
        }
        self.last_value_milli = next.last_value_milli.or(self.last_value_milli);
        self.last_seq = next.last_seq.or(self.last_seq);
        self.last_ms = next.last_ms.or(self.last_ms);
        self.last_playhead = next.last_playhead.or(self.last_playhead);
        self.last_page = next.last_page.or(self.last_page);
        self.last_target_index = next.last_target_index.or(self.last_target_index);
        self.updated_at = Instant::now();

        if let (Some(current), Some(next)) = (self.line.as_object_mut(), next.line.as_object()) {
            for key in [
                "seq",
                "ms",
                "delta_milli",
                "value_milli",
                "outcome",
                "reason",
                "target_index",
                "target_step",
                "pre_target_mask",
                "target_mask",
                "pre_property",
                "property",
                "value_label",
                "step_on",
                "view",
                "overlay",
                "playing",
                "playhead",
                "page",
                "shared_track",
                "shared_mask",
            ] {
                if let Some(value) = next.get(key) {
                    current.insert(key.to_string(), value.clone());
                }
            }
        }
    }

    fn into_line(mut self) -> Value {
        if self.coalesced_count <= 1 {
            return self.line;
        }

        if let Some(obj) = self.line.as_object_mut() {
            obj.insert("coalesced".to_string(), Value::Bool(true));
            obj.insert("count".to_string(), Value::from(self.coalesced_count));
            match self.value_kind {
                EncoderValueKind::Delta => {
                    obj.insert(
                        "delta_milli".to_string(),
                        Value::from(self.total_delta_milli),
                    );
                }
                EncoderValueKind::Absolute => {
                    if let Some(first) = self.first_value_milli {
                        obj.insert("first_value_milli".to_string(), Value::from(first));
                    }
                    if let Some(last) = self.last_value_milli {
                        obj.insert("last_value_milli".to_string(), Value::from(last));
                    }
                }
            }
            if let Some(first_seq) = self.first_seq {
                obj.insert("first_seq".to_string(), Value::from(first_seq));
            }
            if let Some(last_seq) = self.last_seq {
                obj.insert("last_seq".to_string(), Value::from(last_seq));
            }
            if let Some(first_ms) = self.first_ms {
                obj.insert("first_ms".to_string(), Value::from(first_ms));
            }
            if let Some(last_ms) = self.last_ms {
                obj.insert("last_ms".to_string(), Value::from(last_ms));
            }
            if let (Some(first_ms), Some(last_ms)) = (self.first_ms, self.last_ms) {
                obj.insert("duration_ms".to_string(), Value::from(last_ms - first_ms));
            }
            insert_range_if_changed(obj, "playhead", self.first_playhead, self.last_playhead);
            insert_range_if_changed(obj, "page", self.first_page, self.last_page);
            insert_range_if_changed(
                obj,
                "target_index",
                self.first_target_index,
                self.last_target_index,
            );
        }
        self.line
    }

    fn duration_ms(&self) -> Option<i64> {
        Some(self.last_ms? - self.first_ms?)
    }
}

impl EncoderValueKind {
    fn from_object(obj: &Map<String, Value>) -> Option<Self> {
        match obj.get("value_kind").and_then(Value::as_str)? {
            "delta" => Some(Self::Delta),
            "absolute" => Some(Self::Absolute),
            _ => None,
        }
    }

    fn value_milli(self, obj: &Map<String, Value>) -> Option<i64> {
        match self {
            Self::Delta => obj.get("delta_milli").and_then(Value::as_i64),
            Self::Absolute => obj.get("value_milli").and_then(Value::as_i64),
        }
    }
}

fn finalize_event_line(session: &mut ActiveSession, line: Value) -> Value {
    let mut line = line;
    let current_ms = line.get("ms").and_then(Value::as_i64);
    if let Some(obj) = line.as_object_mut() {
        if let (Some(previous), Some(current)) = (session.last_written_ms, current_ms) {
            obj.insert("dt_ms".to_string(), Value::from(current - previous));
        }
    }
    if current_ms.is_some() {
        session.last_written_ms = current_ms;
    }
    line
}

fn append_json_line(path: &Path, value: &Value) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            ApiError::new(
                "io_create_dir_failed",
                format!("create UX recording parent: {e}"),
            )
        })?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| ApiError::new("io_open_failed", format!("open UX recording: {e}")))?;
    serde_json::to_writer(&mut file, value)
        .map_err(|e| ApiError::new("json_write_failed", format!("write UX recording JSON: {e}")))?;
    file.write_all(b"\n").map_err(|e| {
        ApiError::new(
            "io_write_failed",
            format!("write UX recording newline: {e}"),
        )
    })?;
    file.flush()
        .map_err(|e| ApiError::new("io_write_failed", format!("flush UX recording: {e}")))?;
    Ok(())
}

fn update_index_session_started(
    layout: &PayloadLayout,
    session: &ActiveSession,
    trigger: &str,
) -> ApiResult<()> {
    update_index(layout, |index| {
        index.latest = Some(session.path.display().to_string());
        index
            .sessions
            .retain(|entry| entry.path != session.path.display().to_string());
        index.sessions.push(UxRecordingIndexEntry {
            path: session.path.display().to_string(),
            instance_id: session.instance_id.clone(),
            started_at: session.started_at.clone(),
            ended_at: None,
            trigger: trigger.to_string(),
            end_reason: None,
            event_count: 0,
            raw_event_count: 0,
        });
    })
}

fn update_index_counts(
    layout: &PayloadLayout,
    path: &Path,
    event_count: u64,
    raw_event_count: u64,
) -> ApiResult<()> {
    let path = path.display().to_string();
    update_index(layout, |index| {
        if let Some(entry) = index.sessions.iter_mut().find(|entry| entry.path == path) {
            entry.event_count = event_count;
            entry.raw_event_count = raw_event_count;
        }
    })
}

fn update_index_session_ended(
    layout: &PayloadLayout,
    path: &Path,
    ended_at: &str,
    reason: &str,
    event_count: u64,
    raw_event_count: u64,
) -> ApiResult<()> {
    let path = path.display().to_string();
    update_index(layout, |index| {
        if let Some(entry) = index.sessions.iter_mut().find(|entry| entry.path == path) {
            entry.ended_at = Some(ended_at.to_string());
            entry.end_reason = Some(reason.to_string());
            entry.event_count = event_count;
            entry.raw_event_count = raw_event_count;
        }
    })
}

fn update_index<F>(layout: &PayloadLayout, apply: F) -> ApiResult<()>
where
    F: FnOnce(&mut UxRecordingIndex),
{
    let path = layout.ux_recordings_index_file();
    let mut index = read_index(&path)?;
    apply(&mut index);
    if index.sessions.len() > MAX_INDEX_SESSIONS {
        let start = index.sessions.len() - MAX_INDEX_SESSIONS;
        index.sessions = index.sessions[start..].to_vec();
    }
    write_json_atomic(&path, &index)
}

fn read_index(path: &Path) -> ApiResult<UxRecordingIndex> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UxRecordingIndex::default())
        }
        Err(e) => {
            return Err(ApiError::new(
                "io_read_failed",
                format!("read {}: {e}", path.display()),
            ))
        }
    };

    let value: Value = serde_json::from_slice(&bytes).map_err(|e| {
        ApiError::new(
            "json_parse_failed",
            format!("parse {}: {e}", path.display()),
        )
    })?;
    if value.get("schema").and_then(Value::as_u64) != Some(u64::from(UX_RECORDINGS_SCHEMA)) {
        return Ok(UxRecordingIndex::default());
    }
    serde_json::from_value(value).map_err(|e| {
        ApiError::new(
            "json_parse_failed",
            format!("parse {}: {e}", path.display()),
        )
    })
}

fn unique_session_path(dir: &Path, instance_id: &str) -> PathBuf {
    let label = timestamp_label();
    let instance = sanitize_path_segment(instance_id);
    let base = format!("{label}_{instance}_ux");
    let mut path = dir.join(format!("{base}.ndjson"));
    let mut suffix = 1;
    while path.exists() {
        path = dir.join(format!("{base}_{suffix}.ndjson"));
        suffix += 1;
    }
    path
}

fn timestamp_label() -> String {
    Utc::now().format("%Y%m%d-%H%M%S%.3fZ").to_string()
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn sanitize_path_segment(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn state() -> &'static Mutex<UxRecorderState> {
    UX_RECORDER_STATE.get_or_init(|| Mutex::new(UxRecorderState::default()))
}

fn state_mutex() -> std::sync::MutexGuard<'static, UxRecorderState> {
    state().lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_path_segment_keeps_safe_identifier_chars() {
        assert_eq!(
            sanitize_path_segment("bitwig hardware/17081760"),
            "bitwig_hardware_17081760"
        );
    }

    #[test]
    fn pending_encoder_turn_absorbs_metrics() {
        let event = BridgeLogEvent {
            instance_id: Some("instance-a".to_string()),
            port: 9000,
            timestamp: "2026-05-01T09:00:00.000Z".to_string(),
            kind: "debug".to_string(),
            level: Some("info".to_string()),
            message: String::new(),
        };
        let session = ActiveSession {
            instance_id: "instance-a".to_string(),
            path: PathBuf::from("ignored.ndjson"),
            started_at: "2026-05-01T09:00:00.000Z".to_string(),
            event_count: 0,
            raw_event_count: 0,
            last_written_ms: None,
            pending_encoder_turn: None,
            pending_encoder_flush_active: false,
        };
        let first = event_line(
            &session,
            &event,
            serde_json::json!({
                "seq": 1,
                "ms": 1000,
                "kind": "encoder",
                "gesture": "turn",
                "encoder": "NAV",
                "value_kind": "delta",
                "delta_milli": 1000,
                "mode": "sequencer.property_selector",
                "effect": "select_property",
                "target": "property",
                "property": "Gate",
                "pre_view": "sequencer",
                "pre_overlay": "global_settings",
                "view": "sequencer",
                "overlay": "global_settings"
            }),
        );
        let second = event_line(
            &session,
            &BridgeLogEvent {
                timestamp: "2026-05-01T09:00:00.050Z".to_string(),
                ..event.clone()
            },
            serde_json::json!({
                "seq": 2,
                "ms": 1050,
                "kind": "encoder",
                "gesture": "turn",
                "encoder": "NAV",
                "value_kind": "delta",
                "delta_milli": -500,
                "mode": "sequencer.property_selector",
                "effect": "select_property",
                "target": "property",
                "property": "Velocity",
                "pre_view": "sequencer",
                "pre_overlay": "global_settings",
                "view": "sequencer",
                "overlay": "global_settings"
            }),
        );

        let mut pending =
            pending_encoder_turn_from_line(&first, &event).expect("first pending turn");
        pending.absorb(
            pending_encoder_turn_from_line(
                &second,
                &BridgeLogEvent {
                    instance_id: Some("instance-a".to_string()),
                    port: 9000,
                    timestamp: "2026-05-01T09:00:00.050Z".to_string(),
                    kind: "debug".to_string(),
                    level: Some("info".to_string()),
                    message: String::new(),
                },
            )
            .expect("second pending turn"),
        );
        let line = pending.into_line();

        assert_eq!(line["count"], 2);
        assert_eq!(line["delta_milli"], 500);
        assert_eq!(line["first_seq"], 1);
        assert_eq!(line["last_seq"], 2);
        assert_eq!(line["duration_ms"], 50);
        assert_eq!(line["mode"], "sequencer.property_selector");
        assert_eq!(line["effect"], "select_property");
        assert_eq!(line["target"], "property");
        assert_eq!(line["property"], "Velocity");
    }

    #[test]
    fn pending_encoder_turn_preserves_absolute_value_range() {
        let event = BridgeLogEvent {
            instance_id: Some("instance-a".to_string()),
            port: 9000,
            timestamp: "2026-05-01T09:00:00.000Z".to_string(),
            kind: "debug".to_string(),
            level: Some("info".to_string()),
            message: String::new(),
        };
        let session = ActiveSession {
            instance_id: "instance-a".to_string(),
            path: PathBuf::from("ignored.ndjson"),
            started_at: "2026-05-01T09:00:00.000Z".to_string(),
            event_count: 0,
            raw_event_count: 0,
            last_written_ms: None,
            pending_encoder_turn: None,
            pending_encoder_flush_active: false,
        };
        let first = event_line(
            &session,
            &event,
            serde_json::json!({
                "seq": 1,
                "ms": 1000,
                "kind": "encoder",
                "gesture": "turn",
                "encoder": "MACRO_1",
                "value_kind": "absolute",
                "value_milli": 199,
                "pre_view": "macro",
                "pre_overlay": "none",
                "view": "macro",
                "overlay": "none"
            }),
        );
        let second = event_line(
            &session,
            &BridgeLogEvent {
                timestamp: "2026-05-01T09:00:00.050Z".to_string(),
                ..event.clone()
            },
            serde_json::json!({
                "seq": 2,
                "ms": 1050,
                "kind": "encoder",
                "gesture": "turn",
                "encoder": "MACRO_1",
                "value_kind": "absolute",
                "value_milli": 128,
                "pre_view": "macro",
                "pre_overlay": "none",
                "view": "macro",
                "overlay": "none"
            }),
        );

        let mut pending =
            pending_encoder_turn_from_line(&first, &event).expect("first pending turn");
        pending.absorb(
            pending_encoder_turn_from_line(
                &second,
                &BridgeLogEvent {
                    instance_id: Some("instance-a".to_string()),
                    port: 9000,
                    timestamp: "2026-05-01T09:00:00.050Z".to_string(),
                    kind: "debug".to_string(),
                    level: Some("info".to_string()),
                    message: String::new(),
                },
            )
            .expect("second pending turn"),
        );
        let line = pending.into_line();

        assert_eq!(line["count"], 2);
        assert_eq!(line["value_kind"], "absolute");
        assert_eq!(line["value_milli"], 128);
        assert_eq!(line["first_value_milli"], 199);
        assert_eq!(line["last_value_milli"], 128);
        assert!(line.get("delta_milli").is_none());
    }

    #[test]
    fn read_index_resets_incompatible_schema() {
        let path = temp_index_path("incompatible-index.json");
        std::fs::write(
            &path,
            r#"{"schema":1,"latest":null,"sessions":[{"path":"old.ndjson","event_count":1}]}"#,
        )
        .expect("write incompatible index");

        let index = read_index(&path).expect("read index");

        assert_eq!(index.schema, UX_RECORDINGS_SCHEMA);
        assert!(index.sessions.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn read_index_rejects_malformed_current_schema() {
        let path = temp_index_path("malformed-current-index.json");
        std::fs::write(
            &path,
            format!(
                r#"{{"schema":{UX_RECORDINGS_SCHEMA},"latest":null,"sessions":[{{"path":"current.ndjson","instance_id":"device","started_at":"2026-05-01T09:00:00.000Z","ended_at":null,"trigger":"boot","end_reason":null,"event_count":1}}]}}"#
            ),
        )
        .expect("write malformed index");

        let result = read_index(&path);

        assert!(result.is_err());
        let _ = std::fs::remove_file(path);
    }

    fn temp_index_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ms-manager-ux-recorder-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir.join(name)
    }
}
