#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use chrono::{DateTime, Datelike, Duration as ChronoDuration, TimeZone, Utc, Weekday};
use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

const GITHUB_OWNER: &str = "maxacode";
const GITHUB_REPO: &str = "LockPilot-Mac-Win";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TimerAction {
    Popup,
    Lock,
    Shutdown,
    Reboot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum UpdateChannel {
    Main,
    Dev,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RecurrencePreset {
    Daily,
    Weekdays,
    SpecificDays,
    EveryNHours,
    EveryNMinutes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecurrenceConfig {
    preset: RecurrencePreset,
    interval_hours: Option<u32>,
    interval_minutes: Option<u32>,
    days_of_week: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimerInfo {
    id: String,
    action: TimerAction,
    target_time: DateTime<Utc>,
    recurrence: Option<RecurrenceConfig>,
    pre_warning_minutes: Option<Vec<u32>>,
    message: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTimerRequest {
    action: TimerAction,
    target_time: String,
    recurrence: Option<RecurrenceConfig>,
    pre_warning_minutes: Option<Vec<u32>>,
    message: Option<String>,
}

struct TimerEntry {
    info: TimerInfo,
    cancel_tx: mpsc::Sender<()>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PreActionDecision {
    RunNow,
    Snooze10,
    CancelAction,
    ContinueScheduled,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolvePreActionRequest {
    prompt_id: String,
    decision: PreActionDecision,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreActionWarningPayload {
    prompt_id: String,
    timer_id: String,
    action: TimerAction,
    warning_minutes: u32,
    countdown_seconds: u32,
    snooze_minutes: u32,
}

#[derive(Clone)]
struct TimerStore {
    inner: Arc<Mutex<HashMap<String, TimerEntry>>>,
    storage_path: Arc<PathBuf>,
}

#[derive(Clone)]
struct PreActionStore {
    inner: Arc<Mutex<HashMap<String, mpsc::Sender<PreActionDecision>>>>,
}

impl PreActionStore {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl TimerStore {
    fn new(storage_path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            storage_path: Arc::new(storage_path),
        }
    }

    fn persist(&self) -> Result<(), String> {
        let locked = self
            .inner
            .lock()
            .map_err(|_| "Failed to lock timer store".to_string())?;

        let mut timers: Vec<TimerInfo> = locked.values().map(|entry| entry.info.clone()).collect();
        timers.sort_by_key(|timer| timer.target_time);
        drop(locked);

        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Failed to create timer storage directory: {err}"))?;
        }

        let data = serde_json::to_string_pretty(&PersistedTimers { timers })
            .map_err(|err| format!("Failed to encode timer data: {err}"))?;
        fs::write(self.storage_path.as_ref(), data)
            .map_err(|err| format!("Failed to write timer data: {err}"))?;
        Ok(())
    }

    fn load_persisted_infos(&self) -> Result<Vec<TimerInfo>, String> {
        if !self.storage_path.exists() {
            return Ok(Vec::new());
        }

        let raw = fs::read_to_string(self.storage_path.as_ref())
            .map_err(|err| format!("Failed to read timer data: {err}"))?;
        let persisted = serde_json::from_str::<PersistedTimers>(&raw)
            .map_err(|err| format!("Failed to parse timer data: {err}"))?;
        Ok(persisted.timers)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedTimers {
    timers: Vec<TimerInfo>,
}

#[derive(Debug, Deserialize, Clone)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize, Clone)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    draft: bool,
    prerelease: bool,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseVersion {
    tag: String,
    name: String,
    published_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    tag: String,
    name: String,
    notes: Option<String>,
    published_at: Option<String>,
}

// ─── Tauri commands ───────────────────────────────────────────────

#[tauri::command]
fn list_timers(state: State<'_, TimerStore>) -> Result<Vec<TimerInfo>, String> {
    let store = state
        .inner
        .lock()
        .map_err(|_| "Failed to lock timer store".to_string())?;

    let mut timers: Vec<TimerInfo> = store.values().map(|entry| entry.info.clone()).collect();
    timers.sort_by_key(|timer| timer.target_time);

    Ok(timers)
}

#[tauri::command]
fn cancel_timer(id: String, state: State<'_, TimerStore>) -> Result<bool, String> {
    let mut store = state
        .inner
        .lock()
        .map_err(|_| "Failed to lock timer store".to_string())?;

    if let Some(entry) = store.remove(&id) {
        let _ = entry.cancel_tx.send(());
        drop(store);
        state.persist()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
fn resolve_pre_action(
    request: ResolvePreActionRequest,
    state: State<'_, PreActionStore>,
) -> Result<bool, String> {
    let sender = {
        let mut pending = state
            .inner
            .lock()
            .map_err(|_| "Failed to lock pre-action store".to_string())?;
        pending.remove(&request.prompt_id)
    };

    if let Some(tx) = sender {
        let _ = tx.send(request.decision);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
fn create_timer(
    app: tauri::AppHandle,
    request: CreateTimerRequest,
    state: State<'_, TimerStore>,
    pre_action_state: State<'_, PreActionStore>,
) -> Result<TimerInfo, String> {
    let target = DateTime::parse_from_rfc3339(&request.target_time)
        .map_err(|_| "Invalid date/time format".to_string())?
        .with_timezone(&Utc);

    let now = Utc::now();
    if target <= now {
        return Err("Selected time must be in the future".to_string());
    }

    validate_recurrence(request.recurrence.as_ref())?;
    let pre_warning_minutes = normalize_pre_warning_minutes(request.pre_warning_minutes.as_ref())?;

    let id = Uuid::new_v4().to_string();
    let recurrence = request.recurrence.clone();
    let info = TimerInfo {
        id: id.clone(),
        action: request.action,
        target_time: target,
        recurrence: recurrence.clone(),
        pre_warning_minutes: pre_warning_minutes.clone(),
        message: request.message.map(|msg| msg.trim().to_string()),
        created_at: now,
    };

    let (cancel_tx, cancel_rx) = mpsc::channel();

    {
        let mut store = state
            .inner
            .lock()
            .map_err(|_| "Failed to lock timer store".to_string())?;

        store.insert(
            id.clone(),
            TimerEntry {
                info: info.clone(),
                cancel_tx,
            },
        );
    }

    state.persist()?;
    schedule_timer_thread(
        app.clone(),
        pre_action_state.inner.clone(),
        state.inner.clone(),
        state.storage_path.as_ref(),
        id.clone(),
        target,
        info.clone(),
        recurrence,
        cancel_rx,
    );

    Ok(info)
}

fn schedule_timer_thread(
    app: tauri::AppHandle,
    pre_action_store: Arc<Mutex<HashMap<String, mpsc::Sender<PreActionDecision>>>>,
    store: Arc<Mutex<HashMap<String, TimerEntry>>>,
    storage_path: &Path,
    id: String,
    initial_target: DateTime<Utc>,
    task_info: TimerInfo,
    recurrence: Option<RecurrenceConfig>,
    cancel_rx: mpsc::Receiver<()>,
) {
    let storage_path = storage_path.to_path_buf();
    thread::spawn(move || {
        let mut next_run = initial_target;
        let warning_minutes = normalize_pre_warning_minutes(task_info.pre_warning_minutes.as_ref())
            .ok()
            .flatten()
            .unwrap_or_default();
        'timer_loop: loop {

            let mut should_execute_action = true;
            if should_show_pre_action_warning(&task_info.action) && !warning_minutes.is_empty() {
                if let Some(minutes) = warning_minutes.iter().max().copied() {
                    let warning_time = next_run - ChronoDuration::minutes(minutes as i64);
                    let now = Utc::now();
                    if warning_time > now {
                        let wait = match (warning_time - now).to_std() {
                            Ok(duration) => duration,
                            Err(_) => Duration::from_secs(0),
                        };
                        if cancel_rx.recv_timeout(wait).is_ok() {
                            close_pre_action_window(&app, &id);
                            return;
                        }
                    }

                    let decision = request_pre_action_decision(
                        &app,
                        &pre_action_store,
                        &id,
                        &task_info.action,
                        minutes,
                    );
                    match decision {
                        PreActionDecision::RunNow => {
                            close_pre_action_window(&app, &id);
                            run_action(&task_info.action, task_info.message.as_deref());
                            should_execute_action = false;
                        }
                        PreActionDecision::Snooze10 => {
                            close_pre_action_window(&app, &id);
                            next_run = Utc::now() + ChronoDuration::minutes(10);
                            if let Ok(mut locked) = store.lock() {
                                if let Some(entry) = locked.get_mut(&id) {
                                    entry.info.target_time = next_run;
                                } else {
                                    return;
                                }
                            }
                            let _ = persist_inner_store(&store, &storage_path);
                            continue 'timer_loop;
                        }
                        PreActionDecision::CancelAction => {
                            close_pre_action_window(&app, &id);
                            should_execute_action = false;
                        }
                        PreActionDecision::ContinueScheduled => {
                            close_pre_action_window(&app, &id);
                        }
                    }
                }
            }

            if should_execute_action {
                let wait = match (next_run - Utc::now()).to_std() {
                    Ok(duration) => duration,
                    Err(_) => Duration::from_secs(0),
                };
                if cancel_rx.recv_timeout(wait).is_ok() {
                    close_pre_action_window(&app, &id);
                    break;
                }
                close_pre_action_window(&app, &id);
                run_action(&task_info.action, task_info.message.as_deref());
            }

            let Some(recurrence_cfg) = recurrence.as_ref() else {
                if let Ok(mut locked) = store.lock() {
                    locked.remove(&id);
                }
                let _ = persist_inner_store(&store, &storage_path);
                break;
            };

            let computed_next = compute_next_run(next_run, recurrence_cfg);
            let Some(updated_next) = computed_next else {
                if let Ok(mut locked) = store.lock() {
                    locked.remove(&id);
                }
                let _ = persist_inner_store(&store, &storage_path);
                break;
            };
            next_run = updated_next;

            if let Ok(mut locked) = store.lock() {
                if let Some(entry) = locked.get_mut(&id) {
                    entry.info.target_time = next_run;
                } else {
                    break;
                }
            }
            let _ = persist_inner_store(&store, &storage_path);
        }
    });
}

fn should_show_pre_action_warning(action: &TimerAction) -> bool {
    matches!(
        action,
        TimerAction::Lock | TimerAction::Shutdown | TimerAction::Reboot | TimerAction::Popup
    )
}

fn pre_action_window_label(timer_id: &str) -> String {
    format!("prewarning-{timer_id}")
}

fn open_pre_action_window(
    app: &tauri::AppHandle,
    timer_id: &str,
    action: &TimerAction,
    warning_minutes: u32,
    countdown_seconds: u32,
) {
    let label = pre_action_window_label(timer_id);
    if let Some(existing) = app.get_webview_window(&label) {
        let _ = existing.set_focus();
        let _ = existing.show();
        return;
    }

    let action_key = match action {
        TimerAction::Lock => "lock",
        TimerAction::Shutdown => "shutdown",
        TimerAction::Reboot => "reboot",
        TimerAction::Popup => "popup",
    };
    let url = format!(
        "prewarning.html?action={action_key}&warning={warning_minutes}&seconds={countdown_seconds}"
    );

    let _ = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(url.into()))
        .title("LockPilot - Pre Warning")
        .inner_size(420.0, 250.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible(true)
        .focused(true)
        .build();
}

fn close_pre_action_window(app: &tauri::AppHandle, timer_id: &str) {
    let label = pre_action_window_label(timer_id);
    if let Some(window) = app.get_webview_window(&label) {
        let _ = window.close();
    }
}

fn request_pre_action_decision(
    app: &tauri::AppHandle,
    pre_action_store: &Arc<Mutex<HashMap<String, mpsc::Sender<PreActionDecision>>>>,
    timer_id: &str,
    action: &TimerAction,
    warning_minutes: u32,
) -> PreActionDecision {
    let prompt_id = Uuid::new_v4().to_string();
    let (tx, rx) = mpsc::channel::<PreActionDecision>();

    if let Ok(mut pending) = pre_action_store.lock() {
        pending.insert(prompt_id.clone(), tx);
    } else {
        return PreActionDecision::ContinueScheduled;
    }

    let countdown_seconds = warning_minutes.saturating_mul(60).max(1);
    open_pre_action_window(app, timer_id, action, warning_minutes, countdown_seconds);

    let payload = PreActionWarningPayload {
        prompt_id: prompt_id.clone(),
        timer_id: timer_id.to_string(),
        action: action.clone(),
        warning_minutes,
        countdown_seconds,
        snooze_minutes: 10,
    };

    if app.emit("pre_action_warning", payload).is_err() {
        if let Ok(mut pending) = pre_action_store.lock() {
            pending.remove(&prompt_id);
        }
        return PreActionDecision::ContinueScheduled;
    }

    match rx.recv_timeout(Duration::from_secs(countdown_seconds as u64)) {
        Ok(decision) => decision,
        Err(_) => {
            if let Ok(mut pending) = pre_action_store.lock() {
                pending.remove(&prompt_id);
            }
            PreActionDecision::ContinueScheduled
        }
    }
}

fn normalize_pre_warning_minutes(values: Option<&Vec<u32>>) -> Result<Option<Vec<u32>>, String> {
    let Some(values) = values else {
        return Ok(None);
    };

    let mut normalized: Vec<u32> = values
        .iter()
        .copied()
        .filter(|value| matches!(value, 1 | 5 | 10))
        .collect();

    normalized.sort_unstable();
    normalized.dedup();

    if normalized.len() != values.len() {
        return Err("Pre-warning options must be any of: 1, 5, 10 minutes.".to_string());
    }

    Ok(Some(normalized))
}

// ─── Update / release commands ────────────────────────────────────

#[tauri::command]
fn list_release_versions() -> Result<Vec<ReleaseVersion>, String> {
    let mut releases = rollback_releases(fetch_releases()?);
    releases.sort_by(release_version_desc);

    Ok(releases
        .into_iter()
        .map(|release| ReleaseVersion {
            tag: release.tag_name.clone(),
            name: release.name.unwrap_or_else(|| release.tag_name.clone()),
            published_at: release.published_at,
        })
        .collect())
}

#[tauri::command]
fn check_channel_update(
    current_version: String,
    channel: UpdateChannel,
) -> Result<Option<UpdateInfo>, String> {
    let current = normalize_version(&current_version)
        .ok_or_else(|| format!("Invalid current version: {current_version}"))?;

    let mut releases = releases_for_channel(fetch_releases()?, &channel);
    releases.sort_by(release_version_desc);

    let update = releases.into_iter().find(|release| {
        normalize_version(&release.tag_name)
            .map(|version| {
                if version > current {
                    return true;
                }
                
                // Strict semver says 0.4.2-dev.19 < 0.4.2. 
                // However, if the user switches from main (0.4.2) to dev (0.4.2-dev.19), 
                // we want to consider the latest prerelease of the same base version as an update.
                if matches!(channel, UpdateChannel::Dev)
                    && version.major == current.major
                    && version.minor == current.minor
                    && version.patch == current.patch
                    && current.pre.is_empty()
                    && !version.pre.is_empty()
                {
                    return true;
                }
                
                false
            })
            .unwrap_or(false)
    });

    Ok(update.map(|release| UpdateInfo {
        tag: release.tag_name.clone(),
        name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        notes: release.body,
        published_at: release.published_at,
    }))
}

#[tauri::command]
fn install_channel_update(channel: UpdateChannel) -> Result<String, String> {
    let mut releases = releases_for_channel(fetch_releases()?, &channel);
    releases.sort_by(release_version_desc);
    let release = releases
        .into_iter()
        .next()
        .ok_or_else(|| format!("No releases found for {} channel", channel_name(&channel)))?;

    let installer_asset = pick_installer_asset(&release.assets)
        .ok_or_else(|| format!("No installer asset found for release {}", release.tag_name))?;

    let local_installer = download_asset_to_temp(&installer_asset.browser_download_url, &release.tag_name, &installer_asset.name)?;
    open_file(&local_installer)?;

    Ok(format!(
        "Opened {} channel installer {} from {}",
        channel_name(&channel),
        release.tag_name,
        local_installer.display()
    ))
}

#[tauri::command]
fn install_release(tag: String) -> Result<String, String> {
    let releases = rollback_releases(fetch_releases()?);
    let release = releases
        .into_iter()
        .find(|release| tags_match(&release.tag_name, &tag))
        .ok_or_else(|| format!("Release not found for tag: {tag}"))?;

    let installer_asset = pick_installer_asset(&release.assets)
        .ok_or_else(|| format!("No installer asset found for release {}", release.tag_name))?;

    let local_installer = download_asset_to_temp(&installer_asset.browser_download_url, &release.tag_name, &installer_asset.name)?;
    open_file(&local_installer)?;

    Ok(format!(
        "Opened installer for {} from {}",
        release.tag_name,
        local_installer.display()
    ))
}

// ─── Windows system actions ───────────────────────────────────────

fn run_action(action: &TimerAction, message: Option<&str>) {
    match action {
        TimerAction::Popup => {
            let text = message
                .map(str::trim)
                .filter(|msg| !msg.is_empty())
                .unwrap_or("LockPilot timer reached.");
            show_popup(text);
        }
        TimerAction::Lock => {
            lock_workstation();
        }
        TimerAction::Shutdown => {
            let _ = Command::new("shutdown")
                .args(["/s", "/t", "0"])
                .spawn();
        }
        TimerAction::Reboot => {
            let _ = Command::new("shutdown")
                .args(["/r", "/t", "0"])
                .spawn();
        }
    }
}

/// Lock the workstation using the Windows API.
/// On non-Windows platforms this is a no-op (for cross-compilation / type-checking).
#[cfg(windows)]
fn lock_workstation() {
    use windows::Win32::System::Shutdown::LockWorkStation;
    unsafe {
        let _ = LockWorkStation();
    }
}

#[cfg(not(windows))]
fn lock_workstation() {
    eprintln!("lock_workstation: not supported on this platform");
}

/// Show a popup message box.
#[cfg(windows)]
fn show_popup(msg: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_SYSTEMMODAL, MB_TOPMOST,
    };
    use windows::core::PCWSTR;

    let title: Vec<u16> = OsStr::new("LockPilot")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let text: Vec<u16> = OsStr::new(msg)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONINFORMATION | MB_TOPMOST | MB_SETFOREGROUND | MB_SYSTEMMODAL,
        );
    }
}

#[cfg(not(windows))]
fn show_popup(msg: &str) {
    eprintln!("show_popup (stub): {msg}");
}

/// Open a file with the OS default handler.
fn open_file(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/c", "start", "", &path.to_string_lossy()])
            .spawn()
            .map_err(|err| format!("Failed to open file: {err}"))?;
    }
    #[cfg(not(windows))]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|err| format!("Failed to open file: {err}"))?;
    }
    Ok(())
}

// ─── Recurrence helpers ───────────────────────────────────────────

fn validate_recurrence(recurrence: Option<&RecurrenceConfig>) -> Result<(), String> {
    let Some(recurrence) = recurrence else {
        return Ok(());
    };

    match recurrence.preset {
        RecurrencePreset::Daily | RecurrencePreset::Weekdays => Ok(()),
        RecurrencePreset::SpecificDays => {
            let Some(days) = recurrence.days_of_week.as_ref() else {
                return Err("Specific Days requires at least one day.".to_string());
            };
            if days.is_empty() {
                return Err("Specific Days requires at least one day.".to_string());
            }
            if days.len() > 7 {
                return Err("Specific Days can include at most 7 days.".to_string());
            }
            if days.iter().any(|day| parse_weekday(day).is_none()) {
                return Err("Specific Days contains an invalid weekday.".to_string());
            }
            Ok(())
        }
        RecurrencePreset::EveryNHours => {
            let Some(hours) = recurrence.interval_hours else {
                return Err("Every N Hours requires an interval.".to_string());
            };
            if (1..=24).contains(&hours) {
                Ok(())
            } else {
                Err("Interval hours must be between 1 and 24.".to_string())
            }
        }
        RecurrencePreset::EveryNMinutes => {
            let Some(minutes) = recurrence.interval_minutes else {
                return Err("Every N Minutes requires an interval.".to_string());
            };
            if (1..=1440).contains(&minutes) {
                Ok(())
            } else {
                Err("Interval minutes must be between 1 and 1440.".to_string())
            }
        }
    }
}

fn compute_next_run(current_target: DateTime<Utc>, recurrence: &RecurrenceConfig) -> Option<DateTime<Utc>> {
    match recurrence.preset {
        RecurrencePreset::Daily => {
            let mut next = current_target + ChronoDuration::days(1);
            while next <= Utc::now() {
                next += ChronoDuration::days(1);
            }
            Some(next)
        }
        RecurrencePreset::EveryNHours => {
            let interval = recurrence.interval_hours?;
            let mut next = current_target + ChronoDuration::hours(interval as i64);
            while next <= Utc::now() {
                next += ChronoDuration::hours(interval as i64);
            }
            Some(next)
        }
        RecurrencePreset::EveryNMinutes => {
            let interval = recurrence.interval_minutes?;
            let mut next = current_target + ChronoDuration::minutes(interval as i64);
            while next <= Utc::now() {
                next += ChronoDuration::minutes(interval as i64);
            }
            Some(next)
        }
        RecurrencePreset::Weekdays => {
            let time = current_target.time();
            let mut date = current_target.date_naive() + ChronoDuration::days(1);

            for _ in 0..14 {
                let weekday = date.weekday();
                if weekday != Weekday::Sat && weekday != Weekday::Sun {
                    let candidate = Utc.from_utc_datetime(&date.and_time(time));
                    if candidate > Utc::now() {
                        return Some(candidate);
                    }
                }
                date += ChronoDuration::days(1);
            }
            None
        }
        RecurrencePreset::SpecificDays => {
            let allowed_days = recurrence
                .days_of_week
                .as_ref()?
                .iter()
                .filter_map(|day| parse_weekday(day))
                .collect::<Vec<_>>();
            if allowed_days.is_empty() {
                return None;
            }

            let time = current_target.time();
            let mut date = current_target.date_naive() + ChronoDuration::days(1);
            for _ in 0..14 {
                if allowed_days.contains(&date.weekday()) {
                    let candidate = Utc.from_utc_datetime(&date.and_time(time));
                    if candidate > Utc::now() {
                        return Some(candidate);
                    }
                }
                date += ChronoDuration::days(1);
            }
            None
        }
    }
}

fn parse_weekday(input: &str) -> Option<Weekday> {
    match input.trim().to_ascii_lowercase().as_str() {
        "mon" | "monday" => Some(Weekday::Mon),
        "tue" | "tues" | "tuesday" => Some(Weekday::Tue),
        "wed" | "wednesday" => Some(Weekday::Wed),
        "thu" | "thur" | "thurs" | "thursday" => Some(Weekday::Thu),
        "fri" | "friday" => Some(Weekday::Fri),
        "sat" | "saturday" => Some(Weekday::Sat),
        "sun" | "sunday" => Some(Weekday::Sun),
        _ => None,
    }
}

// ─── Persistence helpers ──────────────────────────────────────────

fn persist_inner_store(store: &Arc<Mutex<HashMap<String, TimerEntry>>>, storage_path: &Path) -> Result<(), String> {
    let locked = store
        .lock()
        .map_err(|_| "Failed to lock timer store".to_string())?;
    let mut timers: Vec<TimerInfo> = locked.values().map(|entry| entry.info.clone()).collect();
    timers.sort_by_key(|timer| timer.target_time);
    drop(locked);

    if let Some(parent) = storage_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Failed to create timer storage directory: {err}"))?;
    }

    let data = serde_json::to_string_pretty(&PersistedTimers { timers })
        .map_err(|err| format!("Failed to encode timer data: {err}"))?;
    fs::write(storage_path, data).map_err(|err| format!("Failed to write timer data: {err}"))?;
    Ok(())
}

fn restore_timers(
    store: &TimerStore,
    app: &tauri::AppHandle,
    pre_action_store: &PreActionStore,
) -> Result<(), String> {
    let restored = store.load_persisted_infos()?;
    if restored.is_empty() {
        return Ok(());
    }

    let now = Utc::now();
    for mut info in restored {
        if info.target_time <= now {
            if let Some(recurrence) = info.recurrence.as_ref() {
                let mut next = info.target_time;
                while next <= now {
                    let Some(updated) = compute_next_run(next, recurrence) else {
                        next = now;
                        break;
                    };
                    next = updated;
                }
                if next <= now {
                    continue;
                }
                info.target_time = next;
            } else {
                continue;
            }
        }

        let (cancel_tx, cancel_rx) = mpsc::channel();
        {
            let mut locked = store
                .inner
                .lock()
                .map_err(|_| "Failed to lock timer store".to_string())?;
            locked.insert(
                info.id.clone(),
                TimerEntry {
                    info: info.clone(),
                    cancel_tx,
                },
            );
        }

        schedule_timer_thread(
            app.clone(),
            pre_action_store.inner.clone(),
            store.inner.clone(),
            store.storage_path.as_ref(),
            info.id.clone(),
            info.target_time,
            info.clone(),
            info.recurrence.clone(),
            cancel_rx,
        );
    }

    store.persist()?;
    Ok(())
}

fn timer_storage_path(app: &tauri::AppHandle) -> PathBuf {
    let base = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("lockpilot"));
    base.join("timers.json")
}

// ─── GitHub release helpers ───────────────────────────────────────

fn fetch_releases() -> Result<Vec<GithubRelease>, String> {
    let client = Client::builder()
        .user_agent("LockPilot-Updater")
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {err}"))?;

    let url = format!(
        "https://api.github.com/repos/{}/{}/releases?per_page=100",
        GITHUB_OWNER, GITHUB_REPO
    );

    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("Failed to fetch GitHub releases: {err}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "GitHub releases API returned status {}",
            response.status()
        ));
    }

    response
        .json::<Vec<GithubRelease>>()
        .map_err(|err| format!("Failed to parse GitHub releases: {err}"))
}

fn normalize_version(version: &str) -> Option<Version> {
    Version::parse(version.trim().trim_start_matches('v')).ok()
}

fn release_version_desc(a: &GithubRelease, b: &GithubRelease) -> std::cmp::Ordering {
    let av = normalize_version(&a.tag_name);
    let bv = normalize_version(&b.tag_name);
    bv.cmp(&av)
}

fn rollback_releases(releases: Vec<GithubRelease>) -> Vec<GithubRelease> {
    releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter(|release| normalize_version(&release.tag_name).is_some())
        .filter(|release| has_supported_asset(release))
        .collect()
}

fn releases_for_channel(releases: Vec<GithubRelease>, channel: &UpdateChannel) -> Vec<GithubRelease> {
    let base = releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter(|release| normalize_version(&release.tag_name).is_some())
        .filter(|release| has_supported_asset(release));

    match channel {
        UpdateChannel::Main => base.filter(|release| !release.prerelease).collect(),
        UpdateChannel::Dev => base.filter(|release| release.prerelease).collect(),
    }
}

fn channel_name(channel: &UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Main => "main",
        UpdateChannel::Dev => "dev",
    }
}

fn tags_match(a: &str, b: &str) -> bool {
    a.trim() == b.trim() || a.trim_start_matches('v') == b.trim_start_matches('v')
}

fn has_supported_asset(release: &GithubRelease) -> bool {
    pick_installer_asset(&release.assets).is_some()
}

/// Pick the best Windows installer asset (.msi or .exe) from a release.
fn pick_installer_asset(assets: &[GithubAsset]) -> Option<GithubAsset> {
    let arch = std::env::consts::ARCH;

    // Prefer .msi, then .exe setup files
    let installer_assets: Vec<GithubAsset> = assets
        .iter()
        .filter(|asset| {
            let lower = asset.name.to_lowercase();
            lower.ends_with(".msi") || lower.ends_with(".exe")
        })
        .cloned()
        .collect();

    // Try to match architecture
    let arch_match = match arch {
        "x86_64" => installer_assets
            .iter()
            .find(|asset| {
                let lower = asset.name.to_lowercase();
                lower.contains("x86_64") || lower.contains("x64") || lower.contains("amd64")
            })
            .cloned(),
        "aarch64" => installer_assets
            .iter()
            .find(|asset| {
                let lower = asset.name.to_lowercase();
                lower.contains("aarch64") || lower.contains("arm64")
            })
            .cloned(),
        _ => None,
    };

    arch_match.or_else(|| installer_assets.into_iter().next())
}

/// Download a release asset to a temp file, preserving the file extension.
fn download_asset_to_temp(url: &str, tag: &str, asset_name: &str) -> Result<PathBuf, String> {
    let client = Client::builder()
        .user_agent("LockPilot-Updater")
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {err}"))?;
    let response = client
        .get(url)
        .send()
        .map_err(|err| format!("Failed to download release asset: {err}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Release asset download failed with status {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|err| format!("Failed to read release asset body: {err}"))?;

    // Preserve original file extension (.msi or .exe)
    let extension = Path::new(asset_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("exe");

    let safe_tag = tag.replace('/', "-");
    let path = std::env::temp_dir().join(format!("LockPilot-{safe_tag}.{extension}"));
    fs::write(&path, bytes).map_err(|err| format!("Failed to write installer: {err}"))?;
    Ok(path)
}

// ─── Entry point ──────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let store = TimerStore::new(timer_storage_path(app.handle()));
            let pre_action_store = PreActionStore::new();
            if let Err(err) = restore_timers(&store, app.handle(), &pre_action_store) {
                eprintln!("Failed to restore timers: {err}");
            }
            app.manage(store);
            app.manage(pre_action_store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            create_timer,
            list_timers,
            cancel_timer,
            resolve_pre_action,
            list_release_versions,
            check_channel_update,
            install_channel_update,
            install_release
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
