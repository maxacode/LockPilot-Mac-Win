const { invoke } = window.__TAURI__.core;
const { getVersion } = window.__TAURI__.app;
const { listen } = window.__TAURI__.event;

const form = document.getElementById("timer-form");
const actionInput = document.getElementById("action");
const targetTimeInput = document.getElementById("target-time");
const setNowBtn = document.getElementById("set-now");
const recurrencePresetInput = document.getElementById("recurrence-preset");
const actionChoiceBoxes = document.querySelectorAll(".choice-box[data-action]");
const recurrenceChoiceBoxes = document.querySelectorAll(".choice-box[data-recurrence]");
const quickChipButtons = document.querySelectorAll("[data-quick-minutes]");
const quickCustomInput = document.getElementById("custom-minutes-input");
const quickCustomApplyBtn = document.getElementById("quick-custom-apply");
const preWarnRow = document.getElementById("prewarn-row");
const customPrewarnInput = document.getElementById("custom-prewarn-input");
const prewarnCustomApplyBtn = document.getElementById("prewarn-custom-apply");
const intervalWrap = document.getElementById("interval-wrap");
const intervalHoursInput = document.getElementById("interval-hours");
const specificDaysWrap = document.getElementById("specific-days-wrap");
const specificDayInputs = document.querySelectorAll('input[name="specific-day"]');
const messageWrap = document.getElementById("message-wrap");
const messageInput = document.getElementById("message");
const timersEl = document.getElementById("timers");
const statusEl = document.getElementById("status");
const refreshBtn = document.getElementById("refresh");

const checkUpdatesBtn = document.getElementById("check-updates");
const autoCheckUpdatesInput = document.getElementById("auto-check-updates");
const updateChannelSelect = document.getElementById("update-channel");
const currentVersionEl = document.getElementById("current-version");
const updateStatusEl = document.getElementById("update-status");
const updateResultEl = document.getElementById("update-result");
const latestVersionEl = document.getElementById("latest-version");
const latestNotesEl = document.getElementById("latest-notes");
const installLatestBtn = document.getElementById("install-latest");
const rollbackVersionSelect = document.getElementById("rollback-version");
const rollbackInstallBtn = document.getElementById("rollback-install");
const updateLoadingEl = document.getElementById("update-loading");
const updateLoadingTextEl = document.getElementById("update-loading-text");
const preActionModalEl = document.getElementById("pre-action-modal");
const preActionTitleEl = document.getElementById("pre-action-title");
const preActionLineEl = document.getElementById("pre-action-line");
const preActionSnoozeBtn = document.getElementById("pre-action-snooze");
const preActionSkipBtn = document.getElementById("pre-action-skip");
const preActionRunBtn = document.getElementById("pre-action-run");

const AUTO_UPDATE_KEY = "lockpilot.autoCheckUpdates";
const UPDATE_CHANNEL_KEY = "lockpilot.updateChannel";
let currentVersion = "";
let latestUpdate = null;
let activePreActionPromptId = null;
let activePreActionActionLabel = "";
let activePreActionSnoozeMinutes = 10;
let preActionCountdown = 0;
let preActionCountdownInterval = null;

const getPreWarningInputs = () => document.querySelectorAll('input[name="prewarn"]');

const actionLabel = (action) => {
  if (action === "lock") {
    return "Lock screen";
  }
  if (action === "shutdown") {
    return "Shut down";
  }
  if (action === "reboot") {
    return "Restart";
  }
  return "Action";
};

const renderPreActionLine = () => {
  if (!preActionLineEl) {
    return;
  }
  preActionLineEl.textContent = `${activePreActionActionLabel} in ${Math.max(0, preActionCountdown)}s`;
};

const clearPreActionCountdown = () => {
  if (preActionCountdownInterval) {
    window.clearInterval(preActionCountdownInterval);
    preActionCountdownInterval = null;
  }
};

const closePreActionModal = () => {
  clearPreActionCountdown();
  if (preActionModalEl) {
    preActionModalEl.classList.add("hidden");
  }
  activePreActionPromptId = null;
};

const openPreActionModal = (payload) => {
  activePreActionPromptId = payload.promptId;
  activePreActionActionLabel = actionLabel(payload.action);
  activePreActionSnoozeMinutes = Number(payload.snoozeMinutes || 10);
  preActionCountdown = Number(payload.countdownSeconds || 12);
  if (preActionTitleEl) {
    preActionTitleEl.textContent = `${activePreActionActionLabel} incoming`;
  }
  if (preActionSnoozeBtn) {
    preActionSnoozeBtn.textContent = `Snooze ${activePreActionSnoozeMinutes} min`;
  }
  const warningMinutes = Number(payload.warningMinutes || 0);
  if (preActionLineEl) {
    preActionLineEl.textContent =
      warningMinutes > 0
        ? `${activePreActionActionLabel} in ${warningMinutes} min`
        : `${activePreActionActionLabel} in under 1 min`;
  }
  if (preActionModalEl) {
    preActionModalEl.classList.remove("hidden");
  }

  clearPreActionCountdown();
  preActionCountdownInterval = window.setInterval(() => {
    preActionCountdown -= 1;
    renderPreActionLine();
    if (preActionCountdown <= 0) {
      closePreActionModal();
    }
  }, 1000);
};

const resolvePreAction = async (decision) => {
  if (!activePreActionPromptId) {
    return;
  }
  const promptId = activePreActionPromptId;
  closePreActionModal();
  try {
    await invoke("resolve_pre_action", {
      request: {
        promptId,
        decision,
      },
    });
  } catch (err) {
    showStatus(`Could not resolve warning: ${String(err)}`, true);
  }
};

const showStatus = (text, isError = false) => {
  statusEl.textContent = text;
  statusEl.style.color = isError ? "#c30e2e" : "#4f7480";
};

const showUpdateStatus = (text, isError = false) => {
  updateStatusEl.textContent = text;
  updateStatusEl.style.color = isError ? "#c30e2e" : "#4f7480";
};

const selectedChannel = () => updateChannelSelect.value;

const setUpdateLoading = (loading, text = "Downloading update...") => {
  updateLoadingTextEl.textContent = text;
  updateLoadingEl.classList.toggle("hidden", !loading);
  installLatestBtn.disabled = loading;
  rollbackInstallBtn.disabled = loading;
  checkUpdatesBtn.disabled = loading;
  updateChannelSelect.disabled = loading;
  rollbackVersionSelect.disabled = loading;
};

const toLocalDateTimeValue = (date) => {
  const pad = (n) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
};

const setTriggerToNow = () => {
  targetTimeInput.value = toLocalDateTimeValue(new Date());
};

const parseLocalDateTimeValue = (value) => {
  if (!value) {
    return null;
  }
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
};

const applyQuickIncrement = (minutes, fromNow = false) => {
  const base = parseLocalDateTimeValue(targetTimeInput.value) ?? new Date();
  const start = fromNow ? new Date() : base;
  let next = new Date(base.getTime() + minutes * 60000);
  if (fromNow) {
    next = new Date(start.getTime() + minutes * 60000);
  }
  const minAllowed = new Date(Date.now() + 60000);
  if (next < minAllowed) {
    next = minAllowed;
  }
  targetTimeInput.value = toLocalDateTimeValue(next);
};

const toggleMessage = () => {
  const isPopup = actionInput.value === "popup";
  messageWrap.classList.toggle("is-blank", !isPopup);
  messageInput.required = false;
};

const syncActionChoices = () => {
  actionChoiceBoxes.forEach((box) => {
    box.classList.toggle("is-active", box.dataset.action === actionInput.value);
  });
};

const syncRecurrenceChoices = () => {
  recurrenceChoiceBoxes.forEach((box) => {
    box.classList.toggle("is-active", box.dataset.recurrence === recurrencePresetInput.value);
  });
};

const toggleRecurrence = () => {
  const recurring = recurrencePresetInput.value !== "none";
  const needsInterval =
    recurrencePresetInput.value === "every_n_hours" ||
    recurrencePresetInput.value === "every_n_minutes";
  const needsSpecificDays = recurrencePresetInput.value === "specific_days";
  intervalWrap.classList.toggle("hidden", !needsInterval);
  specificDaysWrap.classList.toggle("hidden", !needsSpecificDays);
  intervalHoursInput.required = needsInterval;
  intervalHoursInput.max = recurrencePresetInput.value === "every_n_minutes" ? "1440" : "24";

  if (!recurring) {
    intervalWrap.classList.add("hidden");
    specificDaysWrap.classList.add("hidden");
  }
};

const fmtDate = (iso) => new Date(iso).toLocaleString();

const fmtRemaining = (iso) => {
  const ms = new Date(iso).getTime() - Date.now();
  if (ms <= 0) {
    return "due now";
  }

  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return `${hours}h ${minutes}m ${seconds}s`;
};

const recurrenceLabel = (recurrence) => {
  if (!recurrence) {
    return "One-time";
  }

  if (recurrence.preset === "daily") {
    return "Repeats daily";
  }

  if (recurrence.preset === "weekdays") {
    return "Repeats weekdays";
  }

  if (recurrence.preset === "specific_days") {
    const shortToLong = {
      mon: "Mon",
      tue: "Tue",
      wed: "Wed",
      thu: "Thu",
      fri: "Fri",
      sat: "Sat",
      sun: "Sun",
    };
    const days = (recurrence.daysOfWeek ?? [])
      .map((day) => shortToLong[day] ?? day)
      .join(", ");
    return days ? `Repeats on ${days}` : "Repeats on specific days";
  }

  if (recurrence.preset === "every_n_hours") {
    return `Repeats every ${recurrence.intervalHours ?? "?"} hour(s)`;
  }

  if (recurrence.preset === "every_n_minutes") {
    return `Repeats every ${recurrence.intervalMinutes ?? "?"} minute(s)`;
  }

  return "Recurring";
};

const renderTimers = (timers) => {
  timersEl.innerHTML = "";

  if (!timers.length) {
    const empty = document.createElement("li");
    empty.className = "empty";
    empty.textContent = "No active timers.";
    timersEl.appendChild(empty);
    return;
  }

  for (const timer of timers) {
    const item = document.createElement("li");
    item.className = "timer-item";

    const top = document.createElement("div");
    top.className = "timer-top";

    const title = document.createElement("strong");
    title.textContent = timer.action.toUpperCase();

    const cancelBtn = document.createElement("button");
    cancelBtn.className = "danger";
    cancelBtn.textContent = "Cancel";
    cancelBtn.addEventListener("click", async () => {
      try {
        await invoke("cancel_timer", { id: timer.id });
        await loadTimers();
        showStatus("Timer canceled.");
      } catch (err) {
        showStatus(String(err), true);
      }
    });

    top.append(title, cancelBtn);

    const when = document.createElement("div");
    when.className = "timer-meta";
    when.textContent = `Runs at ${fmtDate(timer.targetTime)} (${fmtRemaining(timer.targetTime)})`;

    const recurrence = document.createElement("div");
    recurrence.className = "timer-meta";
    recurrence.textContent = recurrenceLabel(timer.recurrence);

    item.append(top, when, recurrence);

    if (timer.action === "popup" && timer.message) {
      const msg = document.createElement("div");
      msg.className = "timer-meta";
      msg.textContent = `Message: ${timer.message}`;
      item.append(msg);
    }

    timersEl.append(item);
  }
};

const loadTimers = async () => {
  try {
    const timers = await invoke("list_timers");
    renderTimers(timers);
  } catch (err) {
    showStatus(String(err), true);
  }
};

const renderUpdateResult = (update) => {
  if (!update) {
    updateResultEl.classList.add("hidden");
    latestUpdate = null;
    return;
  }

  latestUpdate = update;
  updateResultEl.classList.remove("hidden");
  latestVersionEl.textContent = update.tag;
  latestNotesEl.textContent = update.notes?.trim()
    ? update.notes.trim()
    : "What's New:\n- Backend Adjustments\n- Optimizations";
};

const loadRollbackVersions = async () => {
  rollbackVersionSelect.innerHTML = "";

  try {
    const versions = await invoke("list_release_versions");
    versions.forEach((version) => {
      const option = document.createElement("option");
      option.value = version.tag;
      option.textContent = `${version.tag}${version.publishedAt ? ` (${new Date(version.publishedAt).toLocaleDateString()})` : ""}`;
      rollbackVersionSelect.appendChild(option);
    });
  } catch (err) {
    showUpdateStatus(`Could not load release versions: ${String(err)}`, true);
  }
};

const checkForUpdates = async (silentWhenUpToDate = false) => {
  if (!currentVersion) {
    return;
  }

  const channel = selectedChannel();

  try {
    showUpdateStatus(`Checking ${channel} channel on GitHub releases...`);
    const update = await invoke("check_channel_update", {
      currentVersion,
      channel,
    });
    renderUpdateResult(update);

    if (update) {
      showUpdateStatus(`Update available in ${channel}: ${update.tag}`);
    } else if (!silentWhenUpToDate) {
      showUpdateStatus(`No newer version found in ${channel}.`);
    } else {
      showUpdateStatus("");
    }
  } catch (err) {
    showUpdateStatus(`Update check failed: ${String(err)}`, true);
  }
};

const installChannelUpdate = async () => {
  const channel = selectedChannel();

  try {
    setUpdateLoading(true, `Downloading ${channel} update...`);
    await invoke("install_channel_update", { channel });
    showUpdateStatus("Installer opened. Follow the prompts to install the update.");
  } catch (err) {
    showUpdateStatus(`Install failed: ${String(err)}`, true);
  } finally {
    setUpdateLoading(false);
  }
};

const installTag = async (tag) => {
  try {
    setUpdateLoading(true, `Downloading ${tag}...`);
    await invoke("install_release", { tag });
    showUpdateStatus("Installer opened. Follow the prompts to install the update.");
  } catch (err) {
    showUpdateStatus(`Install failed: ${String(err)}`, true);
  } finally {
    setUpdateLoading(false);
  }
};

form.addEventListener("submit", async (event) => {
  event.preventDefault();

  if (!targetTimeInput.value) {
    showStatus("Choose a valid time.", true);
    return;
  }

  const recurrencePreset = recurrencePresetInput.value;
  const selectedSpecificDays = [...specificDayInputs]
    .filter((input) => input.checked)
    .map((input) => input.value);
  const preWarningMinutes = [...getPreWarningInputs()]
    .filter((input) => input.checked)
    .map((input) => Number.parseInt(input.value, 10))
    .filter((value) => Number.isInteger(value) && value > 0);

  if (recurrencePreset === "specific_days" && !selectedSpecificDays.length) {
    showStatus("Select at least one day for Specific Days.", true);
    return;
  }

  let recurrence = null;
  if (recurrencePreset !== "none") {
    recurrence = {
      preset: recurrencePreset,
      intervalHours: recurrencePreset === "every_n_hours" ? Number(intervalHoursInput.value || 0) : null,
      intervalMinutes: recurrencePreset === "every_n_minutes" ? Number(intervalHoursInput.value || 0) : null,
      daysOfWeek: recurrencePreset === "specific_days" ? selectedSpecificDays : null,
    };
  }

  const request = {
    action: actionInput.value,
    targetTime: new Date(targetTimeInput.value).toISOString(),
    recurrence,
    preWarningMinutes,
    message: actionInput.value === "popup" ? messageInput.value : null,
  };

  try {
    const selectedAction = actionInput.value;
    const selectedTargetTime = targetTimeInput.value;
    await invoke("create_timer", { request });
    form.reset();
    actionInput.value = selectedAction;
    targetTimeInput.value = selectedTargetTime;
    recurrencePresetInput.value = "none";
    intervalHoursInput.value = "2";
    specificDayInputs.forEach((input) => {
      input.checked = false;
    });
    toggleMessage();
    toggleRecurrence();
    syncActionChoices();
    syncRecurrenceChoices();
    showStatus("Timer created.");
    await loadTimers();
  } catch (err) {
    showStatus(String(err), true);
  }
});

refreshBtn.addEventListener("click", loadTimers);
actionInput.addEventListener("change", toggleMessage);
recurrencePresetInput.addEventListener("change", toggleRecurrence);
actionInput.addEventListener("change", syncActionChoices);
recurrencePresetInput.addEventListener("change", syncRecurrenceChoices);

actionChoiceBoxes.forEach((box) => {
  box.addEventListener("click", () => {
    const value = box.dataset.action;
    if (!value) {
      return;
    }
    actionInput.value = value;
    toggleMessage();
    syncActionChoices();
  });
});

recurrenceChoiceBoxes.forEach((box) => {
  box.addEventListener("click", () => {
    const value = box.dataset.recurrence;
    if (!value) {
      return;
    }
    recurrencePresetInput.value = value;
    toggleRecurrence();
    syncRecurrenceChoices();
  });
});

quickChipButtons.forEach((chip) => {
  chip.addEventListener("click", () => {
    const minutes = Number(chip.dataset.quickMinutes);
    if (!Number.isInteger(minutes) || minutes <= 0) {
      return;
    }
    applyQuickIncrement(minutes);
  });
});

if (quickCustomApplyBtn) {
  quickCustomApplyBtn.addEventListener("click", () => {
    const minutes = Number.parseInt(String(quickCustomInput?.value || ""), 10);
    if (!Number.isInteger(minutes) || minutes <= 0) {
      showStatus("Custom increment must be a positive number of minutes.", true);
      return;
    }
    applyQuickIncrement(minutes);
  });
}

if (prewarnCustomApplyBtn) {
  prewarnCustomApplyBtn.addEventListener("click", () => {
    const minutes = Number.parseInt(String(customPrewarnInput?.value || ""), 10);
    if (!Number.isInteger(minutes) || minutes <= 0) {
      showStatus("Pre warning custom minutes must be a positive number.", true);
      return;
    }

    const existing = [...getPreWarningInputs()].find((input) => Number.parseInt(input.value, 10) === minutes);
    if (existing) {
      existing.checked = true;
      return;
    }

    if (!preWarnRow) {
      return;
    }

    const label = document.createElement("label");
    label.className = "prewarn-chip";
    label.innerHTML = `<input type=\"checkbox\" name=\"prewarn\" value=\"${minutes}\" checked />${minutes}m`;
    preWarnRow.insertBefore(label, customPrewarnInput);
  });
}

if (setNowBtn) {
  setNowBtn.addEventListener("click", () => {
    setTriggerToNow();
  });
}

if (preActionSnoozeBtn) {
  preActionSnoozeBtn.addEventListener("click", () => {
    resolvePreAction("snooze_10");
  });
}

if (preActionSkipBtn) {
  preActionSkipBtn.addEventListener("click", () => {
    resolvePreAction("cancel_action");
  });
}

if (preActionRunBtn) {
  preActionRunBtn.addEventListener("click", () => {
    resolvePreAction("run_now");
  });
}

checkUpdatesBtn.addEventListener("click", () => checkForUpdates(false));
installLatestBtn.addEventListener("click", installChannelUpdate);

rollbackInstallBtn.addEventListener("click", async () => {
  const selectedTag = rollbackVersionSelect.value;
  if (!selectedTag) {
    showUpdateStatus("Pick a version to install.", true);
    return;
  }

  await installTag(selectedTag);
});

autoCheckUpdatesInput.addEventListener("change", () => {
  localStorage.setItem(AUTO_UPDATE_KEY, autoCheckUpdatesInput.checked ? "1" : "0");
});

updateChannelSelect.addEventListener("change", () => {
  localStorage.setItem(UPDATE_CHANNEL_KEY, selectedChannel());
  renderUpdateResult(null);
});

const initialize = async () => {
  await listen("pre_action_warning", (event) => {
    openPreActionModal(event.payload);
  });

  setTriggerToNow();
  toggleMessage();
  toggleRecurrence();
  syncActionChoices();
  syncRecurrenceChoices();
  await loadTimers();
  setInterval(loadTimers, 1000);

  currentVersion = await getVersion();
  currentVersionEl.textContent = currentVersion;

  const savedChannel = localStorage.getItem(UPDATE_CHANNEL_KEY);
  updateChannelSelect.value = savedChannel === "dev" ? "dev" : "main";

  const autoCheckSetting = localStorage.getItem(AUTO_UPDATE_KEY);
  autoCheckUpdatesInput.checked = autoCheckSetting !== "0";

  await loadRollbackVersions();

  if (autoCheckUpdatesInput.checked) {
    await checkForUpdates(true);
  }
};

initialize().catch((err) => {
  showStatus(`Initialization failed: ${String(err)}`, true);
});
