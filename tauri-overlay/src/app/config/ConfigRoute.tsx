// @ts-nocheck
import * as React from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, emit } from "@tauri-apps/api/event";

import { createLanguageManager } from "../i18n/languageManager";
import GamesTab from "./tabs/GamesTab";
import GenericTab from "./tabs/GenericTab";
import PerformanceTab from "./tabs/PerformanceTab";
import PlayersTab from "./tabs/PlayersTab";
import RandomizerTab from "./tabs/RandomizerTab";
import SettingsTab from "./tabs/SettingsTab";
import StatisticsTab from "./tabs/StatisticsTab";
import WeekliesTab from "./tabs/WeekliesTab";

const { useEffect, useMemo, useRef, useState } = React;

const TABS = [
    {
        id: "settings",
        titleId: "ui_tab_settings",
        groups: [
            {
                title: "General",
                paths: [
                    ["enable_logging"],
                    ["show_player_winrates"],
                    ["show_session"],
                    ["show_random_on_overlay"],
                    ["show_charts"],
                    ["dark_theme"],
                    ["check_for_multiple_instances"],
                ],
            },
            {
                title: "Paths",
                paths: [["account_folder"], ["screenshot_folder"]],
            },
            {
                title: "Overlay",
                paths: [
                    ["monitor"],
                    ["width"],
                    ["height"],
                    ["top_offset"],
                    ["right_offset"],
                    ["subtract_height"],
                    ["duration"],
                    ["font_scale"],
                    ["force_width"],
                ],
            },
            {
                title: "Hotkeys",
                paths: [
                    ["hotkey_show/hide"],
                    ["hotkey_show"],
                    ["hotkey_hide"],
                    ["hotkey_newer"],
                    ["hotkey_older"],
                    ["hotkey_winrates"],
                ],
            },
            {
                title: "Overlay Colors",
                paths: [
                    ["color_player1"],
                    ["color_player2"],
                    ["color_amon"],
                    ["color_mastery"],
                ],
            },
        ],
    },
    {
        id: "games",
        titleId: "ui_tab_games",
        groups: [],
    },
    {
        id: "players",
        titleId: "ui_tab_players",
        groups: [],
    },
    {
        id: "weeklies",
        titleId: "ui_tab_weeklies",
        groups: [],
    },
    {
        id: "statistics",
        titleId: "ui_tab_statistics",
        groups: [],
    },
    {
        id: "randomizer",
        titleId: "ui_tab_randomizer",
        groups: [
            {
                title: "Randomizer",
                paths: [["rng_choices"]],
            },
        ],
    },
    {
        id: "performance",
        titleId: "ui_tab_performance",
        groups: [
            {
                title: "Performance monitor",
                paths: [
                    ["performance_show"],
                    ["performance_hotkey"],
                    ["performance_processes"],
                    ["performance_geometry"],
                ],
            },
        ],
    },
    {
        id: "links",
        titleId: "ui_tab_links",
        groups: [
            {
                title: "Project",
                links: [
                    [
                        "Project - https://github.com/skyser2003/sc2_coop_info",
                        "https://github.com/skyser2003/sc2_coop_info",
                    ],
                    [
                        "Email - sc2coopinfo@gmail.com",
                        "mailto:sc2coopinfo@gmail.com",
                    ],
                ],
            },
        ],
    },
];

const SCO_REPLAY_SCAN_PROGRESS_EVENT = "sco://replay-scan-progress";
const SCO_PERFORMANCE_VISIBILITY_EVENT = "sco://performance-visibility";
const SCO_OVERLAY_COLOR_PREVIEW_EVENT = "sco://overlay-color-preview";
const SCO_OVERLAY_LANGUAGE_PREVIEW_EVENT = "sco://overlay-language-preview";
const SCO_OVERLAY_SCREENSHOT_RESULT_EVENT = "sco://overlay-screenshot-result";
const STATS_DEFAULT_FILTERS = {
    difficulties: {
        Casual: true,
        Normal: true,
        Hard: true,
        Brutal: true,
        BrutalPlus1: true,
        BrutalPlus2: true,
        BrutalPlus3: true,
        BrutalPlus4: true,
        BrutalPlus5: true,
        BrutalPlus6: true,
    },
    regions: {
        NA: true,
        EU: true,
        KR: true,
        CN: true,
    },
    includeNormalGames: true,
    includeMutations: true,
    overrideFolderSelection: true,
    includeMultiBox: false,
    winsOnly: false,
    includeSub15: true,
    includeOver15: true,
    minLength: 0,
    maxLength: 0,
    fromDate: "2015-11-10",
    toDate: "2030-12-30",
    player: "",
};

function cloneJson(value) {
    return JSON.parse(JSON.stringify(value));
}

function getAtPath(source, path) {
    return path.reduce(
        (acc, key) => (acc == null ? undefined : acc[key]),
        source,
    );
}

function setAtPath(source, path, value) {
    const clone = cloneJson(source);
    let cursor = clone;
    for (let i = 0; i < path.length - 1; i += 1) {
        const key = path[i];
        if (
            cursor[key] === undefined ||
            cursor[key] === null ||
            typeof cursor[key] !== "object"
        ) {
            cursor[key] = {};
        }
        cursor = cursor[key];
    }
    cursor[path[path.length - 1]] = value;
    return clone;
}

function performanceVisibilityFromPayload(payload) {
    if (!payload || typeof payload !== "object") {
        return null;
    }
    if (!("visible" in payload)) {
        return null;
    }
    return Boolean(payload.visible);
}

function performanceVisibilityFromSettings(payload) {
    if (!payload || typeof payload !== "object") {
        return null;
    }
    if (!("performance_show" in payload)) {
        return null;
    }
    return Boolean(payload.performance_show);
}

function prettyLabel(value) {
    return value
        .replace(/_/g, " ")
        .replace(/\//g, " / ")
        .replace(/([a-z])([A-Z])/g, "$1 $2")
        .replace(/\b\w/g, (match) => match.toUpperCase());
}

function isSensitivePath(path) {
    const full = path.join(".").toLowerCase();
    return (
        full.includes("secret") ||
        full.includes("oauth") ||
        full.endsWith("password") ||
        full.endsWith("api_key")
    );
}

function asArrayFromText(raw, templateValue) {
    const rows = raw
        .split("\n")
        .map((row) => row.trim())
        .filter((row) => row.length > 0);
    if (
        Array.isArray(templateValue) &&
        templateValue.every((value) => typeof value === "number")
    ) {
        return rows
            .map((value) => Number(value))
            .filter((n) => !Number.isNaN(n));
    }
    return rows;
}

function asTextFromValue(value) {
    if (value === null || value === undefined) {
        return "";
    }
    if (Array.isArray(value)) {
        return value.join("\n");
    }
    if (typeof value === "boolean") {
        return value ? "true" : "false";
    }
    return String(value);
}

function renderNode(value, templateValue, path, depth, onChange) {
    const style = `node-depth-${Math.min(depth, 3)}`;
    const label = path[path.length - 1]
        ? prettyLabel(path[path.length - 1])
        : "Settings";

    if (
        Array.isArray(value) ||
        value === null ||
        value === undefined ||
        typeof value === "boolean" ||
        typeof value === "number" ||
        typeof value === "string"
    ) {
        if (typeof value === "boolean") {
            return (
                <label className="field">
                    <span className="field-label">{label}</span>
                    <input
                        type="checkbox"
                        checked={Boolean(value)}
                        onChange={(event) =>
                            onChange(path, event.target.checked)
                        }
                    />
                </label>
            );
        }

        if (
            Array.isArray(value) ||
            (templateValue && Array.isArray(templateValue))
        ) {
            return (
                <label className="field field-textarea">
                    <span className="field-label">{`${label} (one row per line)`}</span>
                    <textarea
                        rows={Math.max(3, value.length || 3)}
                        className="mono input"
                        value={asTextFromValue(value)}
                        onChange={(event) =>
                            onChange(
                                path,
                                asArrayFromText(
                                    event.target.value,
                                    templateValue,
                                ),
                            )
                        }
                    />
                </label>
            );
        }

        if (typeof value === "number") {
            return (
                <label className="field">
                    <span className="field-label">{label}</span>
                    <input
                        type="number"
                        step="any"
                        value={Number.isFinite(value) ? value : 0}
                        className="input"
                        onChange={(event) =>
                            onChange(path, Number(event.target.value))
                        }
                    />
                </label>
            );
        }

        return (
            <label className="field">
                <span className="field-label">{label}</span>
                <input
                    type={isSensitivePath(path) ? "password" : "text"}
                    value={asTextFromValue(value)}
                    className="input"
                    onChange={(event) => onChange(path, event.target.value)}
                />
            </label>
        );
    }

    if (typeof value === "object") {
        const entries = Object.entries(value);
        return (
            <details className={`${style} card`} open>
                <summary className="section-title">{label}</summary>
                {entries.map(([k, child]) =>
                    renderNode(
                        child,
                        templateValue ? templateValue[k] : undefined,
                        [...path, k],
                        depth + 1,
                        onChange,
                    ),
                )}
            </details>
        );
    }

    return null;
}

function formatPercent(value) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "0.0%";
    }
    return `${(num * 100).toFixed(1)}%`;
}

function normalizeDate(value) {
    if (!value) {
        return "";
    }
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return String(value);
    }
    const ts = num > 1e12 ? num : num * 1000;
    const date = new Date(ts);
    if (Number.isNaN(date.getTime())) {
        return "";
    }
    return date.toLocaleString();
}

function asTableValue(value) {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

function formatPercent0(value) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "-";
    }
    return `${(num * 100).toFixed(0)}%`;
}

function formatPercent1(value) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "-";
    }
    return `${(num * 100).toFixed(1)}%`;
}

function formatNumber(value) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return asTableValue(value);
    }
    return num.toLocaleString("en-US");
}

function formatDurationSeconds(value) {
    const seconds = Number(value);
    if (!Number.isFinite(seconds) || seconds <= 0 || seconds >= 999999) {
        return "-";
    }
    const total = Math.floor(seconds);
    const hh = Math.floor(total / 3600);
    const mm = Math.floor((total % 3600) / 60);
    const ss = total % 60;
    if (hh > 0) {
        return `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
    }
    return `${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
}

function statsFiltersToQuery(filters) {
    const difficultyFilter = [];
    if (!filters.difficulties.Casual) difficultyFilter.push("Casual");
    if (!filters.difficulties.Normal) difficultyFilter.push("Normal");
    if (!filters.difficulties.Hard) difficultyFilter.push("Hard");
    if (!filters.difficulties.Brutal) difficultyFilter.push("Brutal");
    if (!filters.difficulties.BrutalPlus1) {
        difficultyFilter.push("1");
    }
    if (!filters.difficulties.BrutalPlus2) {
        difficultyFilter.push("2");
    }
    if (!filters.difficulties.BrutalPlus3) {
        difficultyFilter.push("3");
    }
    if (!filters.difficulties.BrutalPlus4) {
        difficultyFilter.push("4");
    }
    if (!filters.difficulties.BrutalPlus5) {
        difficultyFilter.push("5");
    }
    if (!filters.difficulties.BrutalPlus6) {
        difficultyFilter.push("6");
    }

    const regionFilter = [];
    if (!filters.regions.NA) regionFilter.push("NA");
    if (!filters.regions.EU) regionFilter.push("EU");
    if (!filters.regions.KR) regionFilter.push("KR");
    if (!filters.regions.CN) regionFilter.push("CN");

    const params = new URLSearchParams();
    params.set("include_mutations", filters.includeMutations ? "1" : "0");
    params.set("include_normal_games", filters.includeNormalGames ? "1" : "0");
    params.set("show_all", filters.overrideFolderSelection ? "1" : "0");
    params.set("wins_only", filters.winsOnly ? "1" : "0");
    params.set("include_both_main", filters.includeMultiBox ? "1" : "0");
    params.set("sub_15", filters.includeSub15 ? "1" : "0");
    params.set("over_15", filters.includeOver15 ? "1" : "0");
    params.set(
        "minlength",
        String(Math.max(0, Number(filters.minLength) || 0)),
    );
    params.set(
        "maxlength",
        String(Math.max(0, Number(filters.maxLength) || 0)),
    );
    params.set("mindate", filters.fromDate || "2015-11-10");
    params.set("maxdate", filters.toDate || "2030-12-30");
    params.set("player", (filters.player || "").trim());
    params.set("difficulty_filter", difficultyFilter.join(","));
    params.set("region_filter", regionFilter.join(","));
    return params.toString();
}

function hotkeyStringFromEvent(event) {
    const baseKey = event.key;
    if (!baseKey) {
        return "";
    }
    if (
        baseKey === "Backspace" ||
        baseKey === "Delete" ||
        baseKey === "Escape" ||
        baseKey === "Esc"
    ) {
        return "";
    }

    const modifiers = [];
    if (event.ctrlKey) modifiers.push("Ctrl");
    if (event.altKey) modifiers.push("Alt");
    if (event.shiftKey) modifiers.push("Shift");
    if (event.metaKey) modifiers.push("Meta");

    const ignored = new Set(["Control", "Shift", "Alt", "Meta"]);
    if (ignored.has(baseKey)) {
        return modifiers.join("+");
    }

    const keyMap = {
        " ": "Space",
        ArrowUp: "Up",
        ArrowDown: "Down",
        ArrowLeft: "Left",
        ArrowRight: "Right",
        PageUp: "PageUp",
        PageDown: "PageDown",
        Home: "Home",
        End: "End",
        Insert: "Insert",
        Enter: "Enter",
        Tab: "Tab",
    };

    const codeMap = {
        Digit0: "0",
        Digit1: "1",
        Digit2: "2",
        Digit3: "3",
        Digit4: "4",
        Digit5: "5",
        Digit6: "6",
        Digit7: "7",
        Digit8: "8",
        Digit9: "9",
        Minus: "-",
        Equal: "=",
        BracketLeft: "[",
        BracketRight: "]",
        Backslash: "\\",
        Semicolon: ";",
        Quote: "'",
        Comma: ",",
        Period: ".",
        Slash: "/",
        Backquote: "`",
        NumpadMultiply: "*",
        NumpadDivide: "/",
        NumpadSubtract: "-",
        NumpadAdd: "+",
        NumpadDecimal: ".",
        NumpadEnter: "Enter",
    };

    let key = codeMap[event.code] || keyMap[baseKey] || baseKey;
    if (key.length === 1 && /[a-z]/i.test(key)) {
        key = key.toUpperCase();
    }
    return [...modifiers, key].join("+");
}

function isHotkeyClearKey(key) {
    return (
        key === "Backspace" ||
        key === "Delete" ||
        key === "Escape" ||
        key === "Esc"
    );
}

function isHotkeyModifierKey(key) {
    return (
        key === "Control" || key === "Shift" || key === "Alt" || key === "Meta"
    );
}

async function requestJson(path, init = {}) {
    const method = init.method || "GET";
    const body = init.body !== undefined ? init.body : null;

    try {
        const payload = await invoke("config_request", {
            path,
            method,
            body,
        });
        if (!payload || payload.status !== "ok") {
            throw new Error(
                payload?.error ||
                    payload?.message ||
                    `Request failed (${method} ${path})`,
            );
        }
        return payload;
    } catch (error) {
        throw error;
    }
}

async function syncHotkeyReassign(currentPath, nextPath) {
    if (currentPath === nextPath) {
        return;
    }

    try {
        if (currentPath !== "") {
            await requestJson("/config/action", {
                method: "POST",
                body: {
                    action: "hotkey_reassign_end",
                    path: currentPath,
                },
            });
        }
        if (nextPath !== "") {
            await requestJson("/config/action", {
                method: "POST",
                body: {
                    action: "hotkey_reassign_begin",
                    path: nextPath,
                },
            });
        }
    } catch (error) {
        console.warn("Failed to sync hotkey reassign state", error);
    }
}

function renderGamesTab(rows, state, languageManager) {
    return (
        <GamesTab
            rows={rows?.rows || rows}
            state={state}
            asTableValue={asTableValue}
            formatDurationSeconds={formatDurationSeconds}
            languageManager={languageManager}
        />
    );
}

function renderPlayersTab(
    rows,
    onRefresh,
    isBusy,
    languageManager,
    playerNotes,
    onPlayerNoteChange,
    onPlayerNoteCommit,
) {
    return (
        <PlayersTab
            rows={rows}
            onRefresh={onRefresh}
            isBusy={isBusy}
            noteValues={playerNotes}
            onNoteChange={onPlayerNoteChange}
            onNoteCommit={onPlayerNoteCommit}
            asTableValue={asTableValue}
            formatPercent={formatPercent}
            languageManager={languageManager}
        />
    );
}

function renderWeekliesTab(rows, onRefresh, isBusy, languageManager) {
    return (
        <WeekliesTab
            rows={rows}
            onRefresh={onRefresh}
            isBusy={isBusy}
            asTableValue={asTableValue}
            formatPercent={formatPercent}
            languageManager={languageManager}
        />
    );
}

function renderStatisticsTab(
    statsPayload,
    statsState,
    actions,
    languageManager,
) {
    return (
        <StatisticsTab
            statsPayload={statsPayload}
            statsState={statsState}
            actions={actions}
            languageManager={languageManager}
        />
    );
}

function renderMainSettingsTab(draft, onChange, actions, languageManager) {
    return (
        <SettingsTab
            draft={draft}
            onChange={onChange}
            getAtPath={getAtPath}
            asTableValue={asTableValue}
            hotkeyStringFromEvent={hotkeyStringFromEvent}
            actions={actions}
            languageManager={languageManager}
        />
    );
}

function renderRandomizerTab(draft, onChange, extraState, languageManager) {
    return (
        <RandomizerTab
            draft={draft}
            onChange={onChange}
            catalog={extraState.randomizerCatalog}
            actions={extraState.randomizerActions}
            languageManager={languageManager}
        />
    );
}

function renderPerformanceTab(draft, onChange, extraState) {
    return (
        <PerformanceTab
            draft={draft}
            onChange={onChange}
            getAtPath={getAtPath}
            actions={extraState.performanceActions}
            displayVisibility={extraState.performanceDisplayVisible}
            languageManager={extraState.languageManager}
        />
    );
}

function renderTabContent(tab, draft, settings, onChange, extraState) {
    if (tab.id === "settings") {
        return renderMainSettingsTab(
            draft,
            onChange,
            extraState.settingsActions,
            extraState.languageManager,
        );
    }
    if (tab.id === "games") {
        return renderGamesTab(
            extraState.tabData.games,
            extraState.gamesState,
            extraState.languageManager,
        );
    }
    if (tab.id === "players") {
        return renderPlayersTab(
            extraState.tabData.players,
            extraState.refreshPlayers,
            extraState.isBusy,
            extraState.languageManager,
            extraState.playerNotes,
            extraState.onPlayerNoteChange,
            extraState.onPlayerNoteCommit,
        );
    }
    if (tab.id === "weeklies") {
        return renderWeekliesTab(
            extraState.tabData.weeklies,
            extraState.refreshWeeklies,
            extraState.isBusy,
            extraState.languageManager,
        );
    }
    if (tab.id === "statistics") {
        return renderStatisticsTab(
            extraState.tabData.statistics,
            extraState.statsState,
            extraState.statsActions,
            extraState.languageManager,
        );
    }
    if (tab.id === "randomizer") {
        return renderRandomizerTab(
            draft,
            onChange,
            extraState,
            extraState.languageManager,
        );
    }
    if (tab.id === "performance") {
        return renderPerformanceTab(draft, onChange, extraState);
    }

    return (
        <GenericTab
            tab={tab}
            draft={draft}
            settings={settings}
            onChange={onChange}
            renderNode={renderNode}
            getAtPath={getAtPath}
        />
    );
}

function SettingsEditor({ onThemeModeChange }) {
    const [settings, setSettings] = useState(null);
    const [draft, setDraft] = useState(null);
    const [status, setStatus] = useState("Loading settings...");
    const [isBusy, setIsBusy] = useState(false);
    const [activeTab, setActiveTab] = useState("settings");
    const [tabData, setTabData] = useState({
        games: null,
        players: null,
        weeklies: null,
        statistics: null,
    });
    const [selectedReplayFile, setSelectedReplayFile] = useState("");
    const [gamesSearch, setGamesSearch] = useState("");
    const [activeHotkeyPath, setActiveHotkeyPath] = useState("");
    const [performanceEditModeEnabled, setPerformanceEditModeEnabled] =
        useState(false);
    const activeHotkeyPathRef = useRef("");
    const hotkeyTransitionRef = useRef(Promise.resolve());
    const [randomizerCatalog, setRandomizerCatalog] = useState(null);
    const [monitorCatalog, setMonitorCatalog] = useState([]);
    const [statsState, setStatsState] = useState({
        filters: cloneJson(STATS_DEFAULT_FILTERS),
        activeSubtab: "maps",
        selectedMap: "",
        selectedMyCommander: "",
        selectedAllyCommander: "",
        selectedUnitMainCommander: "",
        selectedUnitAllyCommander: "",
        selectedUnitSide: "main",
        selectedUnitSortBy: "Unit",
        selectedUnitSortReverse: false,
        amonSearch: "",
    });
    const statsFiltersRef = useRef(cloneJson(STATS_DEFAULT_FILTERS));
    const statsRefreshModeRef = useRef("debounced");
    const statsQueryRef = useRef({
        activeQuery: "",
        desiredQuery: "",
        requestSeq: 0,
        inFlight: false,
        completedAt: 0,
    });
    const startupAnalysisRequestedRef = useRef(false);
    const tabLoadInFlightRef = useRef({
        games: false,
        players: false,
        weeklies: false,
    });
    const gamesLoadLimitRef = useRef(300);
    const draftRef = useRef(null);
    const settingsMutationRef = useRef(Promise.resolve());
    const latestLiveApplySeqRef = useRef(0);
    const liveApplyInFlightRef = useRef(false);
    const queuedLiveApplyRef = useRef(null);
    draftRef.current = draft;
    activeHotkeyPathRef.current = activeHotkeyPath;

    const dirty = useMemo(() => {
        if (settings === null || draft === null) {
            return false;
        }
        return JSON.stringify(settings) !== JSON.stringify(draft);
    }, [settings, draft]);
    const languageManager = useMemo(
        () => createLanguageManager(draft?.language || settings?.language),
        [draft, settings],
    );

    useEffect(() => {
        statsFiltersRef.current = statsState.filters;
    }, [statsState.filters]);

    useEffect(() => {
        return () => {
            if (activeHotkeyPathRef.current !== "") {
                hotkeyTransitionRef.current = hotkeyTransitionRef.current
                    .then(() =>
                        syncHotkeyReassign(activeHotkeyPathRef.current, ""),
                    )
                    .catch((error) => {
                        console.warn(
                            "Failed to clean up hotkey reassign state",
                            error,
                        );
                    });
            }
        };
    }, []);

    useEffect(() => {
        let disposed = false;
        const unlistenPromise = listen(
            SCO_OVERLAY_SCREENSHOT_RESULT_EVENT,
            (event) => {
                if (disposed) {
                    return;
                }
                const payload = event.payload;
                if (
                    payload &&
                    typeof payload === "object" &&
                    "message" in payload &&
                    typeof payload.message === "string"
                ) {
                    setStatus(payload.message);
                }
            },
        );

        return () => {
            disposed = true;
            void unlistenPromise.then((unlisten) => unlisten());
        };
    }, []);

    function transitionHotkeyCapture(nextPath) {
        hotkeyTransitionRef.current = hotkeyTransitionRef.current
            .then(async () => {
                await settingsMutationRef.current;
                const currentPath = activeHotkeyPathRef.current;
                if (currentPath === nextPath) {
                    return;
                }
                await syncHotkeyReassign(currentPath, nextPath);
                activeHotkeyPathRef.current = nextPath;
                setActiveHotkeyPath(nextPath);
            })
            .catch((error) => {
                console.warn("Failed to transition hotkey capture", error);
            });
        return hotkeyTransitionRef.current;
    }

    function beginHotkeyCapture(path) {
        return transitionHotkeyCapture(path);
    }

    function endHotkeyCapture(path) {
        if (activeHotkeyPathRef.current !== path) {
            return Promise.resolve();
        }
        return transitionHotkeyCapture("");
    }

    function applyPerformanceVisibilityState(visible) {
        setSettings((current) =>
            current === null
                ? current
                : setAtPath(current, ["performance_show"], visible),
        );
        setDraft((current) =>
            current === null
                ? current
                : setAtPath(current, ["performance_show"], visible),
        );
        if (!visible) {
            setPerformanceEditModeEnabled(false);
        }
    }

    function safeStatus(message) {
        setStatus(message);
    }

    function replaceDraft(nextDraft) {
        if (
            nextDraft &&
            typeof nextDraft === "object" &&
            "dark_theme" in nextDraft
        ) {
            onThemeModeChange(Boolean(nextDraft.dark_theme));
        }
        draftRef.current = nextDraft;
        setDraft(nextDraft);
    }

    function queueSettingsMutation(task) {
        const run = settingsMutationRef.current.then(task, task);
        settingsMutationRef.current = run.then(
            () => undefined,
            () => undefined,
        );
        return run;
    }

    function cancelPendingLiveApply() {
        queuedLiveApplyRef.current = null;
    }

    function emitOverlayColorPreview(nextSettings) {
        void (async () => {
            try {
                await emit(SCO_OVERLAY_COLOR_PREVIEW_EVENT, {
                    color_player1: getAtPath(nextSettings, ["color_player1"]),
                    color_player2: getAtPath(nextSettings, ["color_player2"]),
                    color_amon: getAtPath(nextSettings, ["color_amon"]),
                    color_mastery: getAtPath(nextSettings, ["color_mastery"]),
                });
            } catch (error) {
                console.warn("Failed to emit overlay color preview", error);
            }
        })();
    }

    function emitOverlayLanguagePreview(nextSettings) {
        void (async () => {
            const emit = await getTauriEmit();
            if (!emit) {
                return;
            }
            try {
                await emit(SCO_OVERLAY_LANGUAGE_PREVIEW_EVENT, {
                    language: getAtPath(nextSettings, ["language"]),
                });
            } catch (error) {
                console.warn("Failed to emit overlay language preview", error);
            }
        })();
    }

    function performRuntimeSettingsApply(
        nextSettings,
        requestSeq,
        successMessage = "Changes applied immediately. Click Save to persist.",
    ) {
        liveApplyInFlightRef.current = true;
        return requestJson("/config", {
            method: "POST",
            body: {
                settings: nextSettings,
                persist: false,
            },
        })
            .then((payload) => {
                setRandomizerCatalog(
                    (current) => payload.randomizer_catalog || current,
                );
                setMonitorCatalog(payload.monitor_catalog || []);
                if (requestSeq === latestLiveApplySeqRef.current) {
                    safeStatus(successMessage);
                }
                return payload;
            })
            .catch((error) => {
                if (requestSeq === latestLiveApplySeqRef.current) {
                    safeStatus(`Failed to apply changes: ${error.message}`);
                }
                return null;
            })
            .finally(() => {
                liveApplyInFlightRef.current = false;
                const queuedApply = queuedLiveApplyRef.current;
                if (
                    queuedApply !== null &&
                    queuedApply.requestSeq > requestSeq
                ) {
                    queuedLiveApplyRef.current = null;
                    void performRuntimeSettingsApply(
                        queuedApply.settings,
                        queuedApply.requestSeq,
                        queuedApply.successMessage,
                    );
                }
            });
    }

    function applyRuntimeSettings(
        nextSettings,
        successMessage = "Changes applied immediately. Click Save to persist.",
    ) {
        const requestSeq = latestLiveApplySeqRef.current + 1;
        latestLiveApplySeqRef.current = requestSeq;
        if (liveApplyInFlightRef.current) {
            queuedLiveApplyRef.current = {
                settings: nextSettings,
                requestSeq,
                successMessage,
            };
            return Promise.resolve(null);
        }
        return performRuntimeSettingsApply(
            nextSettings,
            requestSeq,
            successMessage,
        );
    }

    async function loadSettings() {
        try {
            cancelPendingLiveApply();
            setIsBusy(true);
            const payload = await requestJson("/config");
            if (!payload.settings) {
                throw new Error("Invalid response from API");
            }
            const activeSettings = payload.active_settings || payload.settings;
            setSettings(payload.settings);
            replaceDraft(activeSettings);
            setRandomizerCatalog(payload.randomizer_catalog || null);
            setMonitorCatalog(payload.monitor_catalog || []);
            setStatus("Settings loaded");
        } catch (error) {
            setStatus(`Failed to load settings: ${error.message}`);
        } finally {
            setIsBusy(false);
        }
    }

    useEffect(() => {
        loadSettings();
    }, []);

    useEffect(() => {
        if (draft === null || startupAnalysisRequestedRef.current) {
            return;
        }
        startupAnalysisRequestedRef.current = true;
        void requestJson("/config/stats/action", {
            method: "POST",
            body: { action: "frontend_ready" },
        }).catch((error) => {
            console.warn("Failed to trigger startup analysis", error);
        });
    }, [draft]);

    useEffect(() => {
        let isMounted = true;
        let unlisten = null;
        (async () => {
            if (!isMounted) {
                return;
            }

            try {
                unlisten = await listen(
                    SCO_REPLAY_SCAN_PROGRESS_EVENT,
                    (event) => {
                        if (!isMounted) {
                            return;
                        }
                        const progress = event?.payload;
                        if (!progress || typeof progress !== "object") {
                            return;
                        }
                        setTabData((current) => ({
                            ...current,
                            statistics: {
                                ...(current.statistics || {}),
                                scan_progress: progress,
                            },
                        }));
                    },
                );
            } catch (error) {
                console.warn(
                    "Failed to subscribe to scan progress events",
                    error,
                );
            }
        })();

        return () => {
            isMounted = false;
            if (typeof unlisten === "function") {
                unlisten();
            }
        };
    }, []);

    useEffect(() => {
        let isMounted = true;
        let unlisten = null;
        (async () => {
            if (!isMounted) {
                return;
            }

            try {
                unlisten = await listen(
                    SCO_PERFORMANCE_VISIBILITY_EVENT,
                    (event) => {
                        if (!isMounted) {
                            return;
                        }
                        const visible = performanceVisibilityFromPayload(
                            event?.payload,
                        );
                        if (visible === null) {
                            return;
                        }
                        applyPerformanceVisibilityState(visible);
                        void requestJson("/config")
                            .then((payload) => {
                                const confirmedVisible =
                                    performanceVisibilityFromSettings(
                                        payload?.active_settings ||
                                            payload?.settings,
                                    );
                                if (!isMounted || confirmedVisible === null) {
                                    return;
                                }
                                applyPerformanceVisibilityState(
                                    confirmedVisible,
                                );
                            })
                            .catch((error) => {
                                console.warn(
                                    "Failed to reconcile performance visibility state",
                                    error,
                                );
                            });
                    },
                );
            } catch (error) {
                console.warn(
                    "Failed to subscribe to performance visibility events",
                    error,
                );
            }
        })();

        return () => {
            isMounted = false;
            if (typeof unlisten === "function") {
                unlisten();
            }
        };
    }, []);

    useEffect(() => {
        const runtimeWindow = window;
        runtimeWindow.__scoSetPerformanceVisibility = (visible) => {
            applyPerformanceVisibilityState(Boolean(visible));
        };
        return () => {
            delete runtimeWindow.__scoSetPerformanceVisibility;
        };
    }, []);

    function getPayloadForTab(tabId, payload) {
        if (tabId === "games") {
            return {
                rows: payload.replays || [],
                totalRows:
                    Number(payload.total_replays) ||
                    (payload.replays || []).length,
            };
        }
        if (tabId === "players") return payload.players || [];
        if (tabId === "weeklies") return payload.weeklies || [];
        if (tabId === "statistics") return payload.stats || null;
        return null;
    }

    async function loadTabData(tabId, force = false, options = {}) {
        if (!["games", "players", "weeklies"].includes(tabId)) {
            return;
        }
        if (!force && tabLoadInFlightRef.current[tabId]) {
            return;
        }
        tabLoadInFlightRef.current[tabId] = true;
        try {
            setIsBusy(true);
            const gamesLimit =
                Number(options.gamesLimit) > 0
                    ? Number(options.gamesLimit)
                    : gamesLoadLimitRef.current;
            if (tabId === "games") {
                gamesLoadLimitRef.current = gamesLimit;
            }
            const urlMap = {
                games: `/config/replays?limit=${gamesLimit}`,
                players: "/config/players?limit=500",
                weeklies: "/config/weeklies",
            };
            const payload = await requestJson(urlMap[tabId]);
            setTabData((current) => ({
                ...current,
                [tabId]: getPayloadForTab(tabId, payload),
            }));
            safeStatus(`${tabId} refreshed`);
        } catch (error) {
            safeStatus(`Failed to load ${tabId}: ${error.message}`);
        } finally {
            tabLoadInFlightRef.current[tabId] = false;
            setIsBusy(false);
        }
    }

    useEffect(() => {
        if (activeTab === "games" && tabData.games === null) {
            loadTabData("games");
            return;
        }
        if (activeTab === "players" && tabData.players === null) {
            loadTabData("players");
            return;
        }
        if (activeTab === "weeklies" && tabData.weeklies === null) {
            loadTabData("weeklies");
            return;
        }
        if (activeTab === "statistics" && tabData.statistics === null) {
            refreshStatistics(true);
        }
    }, [
        activeTab,
        tabData.games,
        tabData.players,
        tabData.weeklies,
        tabData.statistics,
    ]);

    async function postAction(path, payload) {
        setIsBusy(true);
        try {
            const result = await requestJson(path, {
                method: "POST",
                body: payload,
            });
            safeStatus(result.message || "Action completed");
            return result;
        } catch (error) {
            safeStatus(`Action failed: ${error.message}`);
            return null;
        } finally {
            setIsBusy(false);
        }
    }

    function updateField(path, value) {
        if (draftRef.current === null) {
            return;
        }
        const nextDraft = setAtPath(draftRef.current, path, value);
        replaceDraft(nextDraft);
        const isColorField =
            path.length === 1 &&
            (path[0] === "color_player1" ||
                path[0] === "color_player2" ||
                path[0] === "color_amon" ||
                path[0] === "color_mastery");
        if (isColorField) {
            emitOverlayColorPreview(nextDraft);
        }
        if (path.length === 1 && path[0] === "language") {
            emitOverlayLanguagePreview(nextDraft);
        }
        if (isColorField) {
            return;
        }
        cancelPendingLiveApply();
        void applyRuntimeSettings(nextDraft);
    }

    function normalizePlayerNoteKey(value) {
        return asTableValue(value).trim().toLowerCase();
    }

    function patchedPlayerNotes(currentSettings, handle, noteValue) {
        const currentNotesValue = getAtPath(currentSettings, ["player_notes"]);
        const currentNotes =
            currentNotesValue &&
            typeof currentNotesValue === "object" &&
            !Array.isArray(currentNotesValue)
                ? { ...currentNotesValue }
                : {};
        const normalizedHandle = normalizePlayerNoteKey(handle);
        if (normalizedHandle === "") {
            return;
        }

        const existingKey =
            Object.keys(currentNotes).find(
                (key) => normalizePlayerNoteKey(key) === normalizedHandle,
            ) || handle;
        const trimmed = asTableValue(noteValue).trim();
        if (trimmed === "") {
            delete currentNotes[existingKey];
        } else {
            currentNotes[existingKey] = noteValue;
        }

        return currentNotes;
    }

    function updatePlayerNote(handle, noteValue) {
        setDraft((current) => {
            if (current === null) {
                return current;
            }
            return setAtPath(
                current,
                ["player_notes"],
                patchedPlayerNotes(current, handle, noteValue),
            );
        });
    }

    async function persistPlayerNote(handle, noteValue) {
        try {
            setIsBusy(true);
            const payload = await requestJson("/config/action", {
                method: "POST",
                body: {
                    action: "set_player_note",
                    player: handle,
                    note: noteValue,
                },
            });
            setSettings((current) => {
                if (current === null) {
                    return current;
                }
                return setAtPath(
                    current,
                    ["player_notes"],
                    patchedPlayerNotes(current, handle, noteValue),
                );
            });
            setStatus(payload.message || "Player note saved");
        } catch (error) {
            setStatus(`Failed to save player note: ${error.message}`);
        } finally {
            setIsBusy(false);
        }
    }

    async function saveProvidedSettings(nextSettings) {
        cancelPendingLiveApply();
        await queueSettingsMutation(async () => {
            try {
                setIsBusy(true);
                const payload = await requestJson("/config", {
                    method: "POST",
                    body: {
                        settings: nextSettings,
                        persist: true,
                    },
                });
                const activeSettings =
                    payload.active_settings || payload.settings;
                setSettings(payload.settings);
                replaceDraft(activeSettings);
                setRandomizerCatalog(
                    (current) => payload.randomizer_catalog || current,
                );
                setMonitorCatalog(payload.monitor_catalog || []);
                setStatus("Saved to settings.json");
            } catch (error) {
                setStatus(`Failed to save: ${error.message}`);
            } finally {
                setIsBusy(false);
            }
        });
    }

    async function saveSettings() {
        if (draftRef.current === null) {
            return;
        }
        await saveProvidedSettings(draftRef.current);
    }

    function resetSettings() {
        if (settings !== null) {
            const nextDraft = cloneJson(settings);
            replaceDraft(nextDraft);
            cancelPendingLiveApply();
            emitOverlayColorPreview(nextDraft);
            void applyRuntimeSettings(nextDraft, "Reverted to saved settings.");
        }
    }

    async function showSelectedReplay() {
        if (!selectedReplayFile) {
            setStatus("Select a replay first");
            return;
        }
        const result = await postAction("/config/replays/show", {
            file: selectedReplayFile,
        });
        if (result) {
            setStatus("Replay sent to overlay");
            await loadTabData("games");
        }
    }

    async function showReplayByFile(file) {
        if (!file) {
            return;
        }
        setSelectedReplayFile(file);
        const result = await postAction("/config/replays/show", { file });
        if (result) {
            setStatus("Replay sent to overlay");
        }
    }

    async function loadReplayChat(file) {
        if (!file) {
            return null;
        }
        const result = await requestJson("/config/replays/chat", {
            method: "POST",
            body: { file },
        });
        return result.chat || null;
    }

    async function revealReplayByFile(file) {
        if (!file) {
            return;
        }
        await postAction("/config/stats/action", {
            action: "reveal_file",
            file,
        });
    }

    async function moveReplay(delta) {
        const result = await postAction("/config/replays/move", { delta });
        if (result) {
            await loadTabData("games", false, {
                gamesLimit: gamesLoadLimitRef.current,
            });
        }
    }

    async function postConfigAction(action, payload = {}) {
        return postAction("/config/action", { action, ...payload });
    }

    async function promptPath(path, title) {
        const current = asTableValue(getAtPath(draftRef.current, path)).trim();

        try {
            setIsBusy(true);
            const selected = await invoke("pick_folder", {
                title,
                directory: current === "" ? null : current,
            });
            if (typeof selected !== "string") {
                return;
            }
            const normalized = selected.trim();
            if (normalized === "") {
                return;
            }
            if (draftRef.current === null) {
                return;
            }
            const nextDraft = setAtPath(draftRef.current, path, normalized);
            replaceDraft(nextDraft);
            cancelPendingLiveApply();
            void applyRuntimeSettings(
                nextDraft,
                "Folder selected and applied. Click Save to persist.",
            );
        } catch (error) {
            safeStatus(`Failed to select folder: ${error.message}`);
        } finally {
            setIsBusy(false);
        }
    }

    async function triggerOverlayAction(actionName) {
        const result = await postConfigAction(actionName);
        if (!result) {
            return;
        }
        if (actionName === "performance_toggle_reposition") {
            setPerformanceEditModeEnabled((current) => !current);
        }
    }

    async function createDesktopShortcut() {
        await postConfigAction("create_desktop_shortcut");
    }

    async function parseReplayPrompt() {
        const suggested = selectedReplayFile || "";
        const value = window.prompt(
            "Replay file path (*.SC2Replay)",
            suggested,
        );
        if (value === null || value.trim() === "") {
            return;
        }
        await postConfigAction("parse_replay", { file: value.trim() });
    }

    async function overlayScreenshot() {
        await postConfigAction("overlay_screenshot");
    }

    async function openFolderPath(path) {
        const normalized = String(path || "").trim();
        if (normalized === "") {
            safeStatus("Folder path is empty");
            return null;
        }

        setIsBusy(true);
        try {
            await invoke("open_folder_path", {
                path: normalized,
            });
            safeStatus(`Opened folder: ${normalized}`);
            return true;
        } catch (error) {
            safeStatus(`Failed to open folder: ${error.message}`);
            return null;
        } finally {
            setIsBusy(false);
        }
    }

    function applyMainSettings() {
        saveSettings();
    }

    function resetMainSettings() {
        resetSettings();
    }

    async function refreshStatistics(
        silent = false,
        customFilters = null,
        force = false,
    ) {
        const filters = customFilters || statsState.filters;
        const query = statsFiltersToQuery(filters);
        const existingQuery = tabData.statistics && tabData.statistics.query;
        const now = Date.now();
        const completedQuery = statsQueryRef.current;
        statsQueryRef.current = {
            ...completedQuery,
            desiredQuery: query,
        };

        if (
            !force &&
            !customFilters &&
            existingQuery &&
            existingQuery === query &&
            !completedQuery.inFlight &&
            now - completedQuery.completedAt < 3000
        ) {
            return;
        }
        if (completedQuery.inFlight) {
            return;
        }

        const requestSeq = completedQuery.requestSeq + 1;
        statsQueryRef.current = {
            ...statsQueryRef.current,
            requestSeq,
            activeQuery: query,
            inFlight: true,
        };

        try {
            setIsBusy(true);
            const payload = await requestJson(`/config/stats?${query}`);
            if (
                statsQueryRef.current.requestSeq !== requestSeq ||
                statsQueryRef.current.activeQuery !== query
            ) {
                return;
            }
            setTabData((current) => ({
                ...current,
                statistics: getPayloadForTab("statistics", payload),
            }));
            statsQueryRef.current = {
                ...statsQueryRef.current,
                inFlight: false,
                completedAt: Date.now(),
            };
            if (!silent) {
                safeStatus("statistics refreshed");
            }
        } catch (error) {
            if (statsQueryRef.current.requestSeq !== requestSeq) {
                return;
            }
            statsQueryRef.current = {
                ...statsQueryRef.current,
                inFlight: false,
                completedAt: Date.now(),
            };
            safeStatus(`Failed to load statistics: ${error.message}`);
        } finally {
            if (statsQueryRef.current.requestSeq === requestSeq) {
                const desiredQuery = statsQueryRef.current.desiredQuery;
                const needsFollowup =
                    typeof desiredQuery === "string" &&
                    desiredQuery.length > 0 &&
                    desiredQuery !== query;
                statsQueryRef.current = {
                    ...statsQueryRef.current,
                    inFlight: false,
                    completedAt: Date.now(),
                };
                if (needsFollowup) {
                    setTimeout(() => {
                        refreshStatistics(true, statsFiltersRef.current, true);
                    }, 0);
                } else {
                    setIsBusy(false);
                }
            }
        }
    }

    async function startSimpleAnalysis() {
        const result = await postAction("/config/stats/action", {
            action: "start_simple_analysis",
        });
        if (result) {
            setTimeout(() => refreshStatistics(true, null, true), 800);
        }
    }

    async function runDetailedAnalysis() {
        const result = await postAction("/config/stats/action", {
            action: "run_detailed_analysis",
        });
        if (result) {
            setTimeout(() => refreshStatistics(true, null, true), 800);
        }
    }

    async function pauseDetailedAnalysis() {
        const result = await postAction("/config/stats/action", {
            action: "pause_detailed_analysis",
        });
        if (result) {
            setTimeout(() => refreshStatistics(true, null, true), 300);
        }
    }

    async function dumpData() {
        await postAction("/config/stats/action", { action: "dump_data" });
    }

    async function deleteParsedData() {
        const result = await postAction("/config/stats/action", {
            action: "delete_parsed_data",
        });
        if (result) {
            setTimeout(() => refreshStatistics(true, null, true), 1000);
        }
    }

    async function setDetailedAnalysisAtStart(enabled) {
        const result = await postAction("/config/stats/action", {
            action: "set_detailed_analysis_atstart",
            enabled: Boolean(enabled),
        });
        if (result) {
            setTabData((current) => ({
                ...current,
                statistics: current.statistics
                    ? {
                          ...current.statistics,
                          detailed_analysis_atstart: Boolean(enabled),
                      }
                    : current.statistics,
            }));
        }
    }

    async function revealReplay(file) {
        if (!file) {
            return;
        }
        await postAction("/config/stats/action", {
            action: "reveal_file",
            file,
        });
    }

    async function showReplay(file) {
        if (!file) {
            return;
        }
        await postAction("/config/replays/show", { file });
    }

    function setStatsBool(key) {
        const nextFilters = {
            ...statsFiltersRef.current,
            [key]: !statsFiltersRef.current[key],
        };
        statsRefreshModeRef.current = "immediate";
        statsFiltersRef.current = nextFilters;
        setStatsState((current) => ({
            ...current,
            filters: nextFilters,
        }));
    }

    function setStatsText(key, value) {
        const nextFilters = {
            ...statsFiltersRef.current,
            [key]: value,
        };
        statsRefreshModeRef.current = "debounced";
        statsFiltersRef.current = nextFilters;
        setStatsState((current) => ({
            ...current,
            filters: nextFilters,
        }));
    }

    function setStatsNumber(key, value) {
        const parsed = Number(value);
        const nextFilters = {
            ...statsFiltersRef.current,
            [key]: Number.isFinite(parsed) ? Math.max(0, parsed) : 0,
        };
        statsRefreshModeRef.current = "debounced";
        statsFiltersRef.current = nextFilters;
        setStatsState((current) => ({
            ...current,
            filters: nextFilters,
        }));
    }

    function toggleDifficulty(key) {
        const nextFilters = {
            ...statsFiltersRef.current,
            difficulties: {
                ...statsFiltersRef.current.difficulties,
                [key]: !statsFiltersRef.current.difficulties[key],
            },
        };
        statsRefreshModeRef.current = "immediate";
        statsFiltersRef.current = nextFilters;
        setStatsState((current) => ({
            ...current,
            filters: nextFilters,
        }));
    }

    function toggleRegion(key) {
        const nextFilters = {
            ...statsFiltersRef.current,
            regions: {
                ...statsFiltersRef.current.regions,
                [key]: !statsFiltersRef.current.regions[key],
            },
        };
        statsRefreshModeRef.current = "immediate";
        statsFiltersRef.current = nextFilters;
        setStatsState((current) => ({
            ...current,
            filters: nextFilters,
        }));
    }

    const observesStatistics =
        activeTab === "statistics" || activeTab === "settings";

    useEffect(() => {
        if (!observesStatistics) {
            return;
        }

        const mapData = tabData.statistics?.analysis?.MapData;
        if (!mapData || typeof mapData !== "object") {
            return;
        }

        const selectedMap = statsState.selectedMap;
        if (!selectedMap) {
            return;
        }

        if (Object.prototype.hasOwnProperty.call(mapData, selectedMap)) {
            return;
        }

        setStatsState((current) => {
            if (!current.selectedMap) {
                return current;
            }
            return {
                ...current,
                selectedMap: "",
            };
        });
    }, [observesStatistics, statsState.selectedMap, tabData.statistics]);

    useEffect(() => {
        if (!observesStatistics) {
            return undefined;
        }
        if (tabData.statistics === null) {
            refreshStatistics(true, null, true);
            return undefined;
        }
        const currentQuery = statsFiltersToQuery(statsState.filters);
        const hasCachedQuery =
            tabData.statistics && typeof tabData.statistics.query === "string";
        if (
            hasCachedQuery &&
            tabData.statistics.query === currentQuery &&
            !tabData.statistics.simple_analysis_running &&
            !tabData.statistics.detailed_analysis_running
        ) {
            return undefined;
        }
        const refreshDelayMs =
            statsRefreshModeRef.current === "immediate" ? 0 : 250;
        statsRefreshModeRef.current = "debounced";
        const timer = setTimeout(() => {
            refreshStatistics(true);
        }, refreshDelayMs);
        return () => clearTimeout(timer);
    }, [observesStatistics, statsState.filters]);

    useEffect(() => {
        if (!observesStatistics) {
            return undefined;
        }
        if (!tabData.statistics) {
            refreshStatistics(true, null, true);
            return undefined;
        }
        const isParsing =
            !tabData.statistics.ready ||
            tabData.statistics.simple_analysis_running ||
            tabData.statistics.detailed_analysis_running;

        if (!isParsing) {
            return undefined;
        }
        const timer = setInterval(() => {
            refreshStatistics(true, null, true);
        }, 2000);
        return () => clearInterval(timer);
    }, [
        observesStatistics,
        tabData.statistics && tabData.statistics.ready,
        tabData.statistics && tabData.statistics.simple_analysis_running,
        tabData.statistics && tabData.statistics.detailed_analysis_running,
        statsState.filters,
    ]);

    function refreshDataTabOnClick(tabId) {
        if (tabId === "games") {
            loadTabData("games");
            return;
        }
        if (tabId === "players") {
            loadTabData("players");
            return;
        }
        if (tabId === "weeklies") {
            loadTabData("weeklies");
            return;
        }
        if (tabId === "statistics") {
            refreshStatistics(true, null, true);
            return;
        }
        if (tabId === "settings") {
            refreshStatistics(true, null, true);
        }
    }

    const active = TABS.find((tab) => tab.id === activeTab) || TABS[0];
    const tabContent =
        draft === null ? (
            <section className="tab-content">
                <div className="card group">
                    <p {...null}>{status}</p>
                </div>
            </section>
        ) : (
            renderTabContent(active, draft, settings, updateField, {
                tabData,
                isBusy,
                settingsActions: {
                    isBusy,
                    ready: tabData.statistics?.ready,
                    hasPendingChanges: dirty,
                    promptPath,
                    openFolderPath,
                    triggerOverlayAction,
                    activeHotkeyPath,
                    beginHotkeyCapture,
                    endHotkeyCapture,
                    createDesktopShortcut,
                    parseReplayPrompt,
                    overlayScreenshot,
                    runDetailedAnalysis,
                    startSimpleAnalysis,
                    pauseDetailedAnalysis,
                    deleteParsedData: async () => {
                        await deleteParsedData();
                    },
                    applyMainSettings,
                    resetMainSettings,
                    monitorOptions: monitorCatalog,
                    isHotkeyClearKey,
                    isHotkeyModifierKey,
                    simpleAnalysisRunning: Boolean(
                        tabData.statistics?.simple_analysis_running,
                    ),
                    simpleAnalysisStatus:
                        tabData.statistics?.simple_analysis_status || "",
                    detailedAnalysisRunning: Boolean(
                        tabData.statistics?.detailed_analysis_running,
                    ),
                    detailedAnalysisStatus:
                        tabData.statistics?.detailed_analysis_status || "",
                    analysisMessage: tabData.statistics?.message || "",
                    analysisScanProgress:
                        tabData.statistics?.scan_progress ?? null,
                    analysisTotalValidFiles: Number(
                        tabData.statistics?.total_valid_files ?? 0,
                    ),
                    analysisDetailedParsedCount: Number(
                        tabData.statistics?.detailed_parsed_count ?? 0,
                    ),
                },
                refreshPlayers: () => loadTabData("players"),
                playerNotes:
                    draft &&
                    draft.player_notes &&
                    typeof draft.player_notes === "object" &&
                    !Array.isArray(draft.player_notes)
                        ? draft.player_notes
                        : {},
                onPlayerNoteChange: updatePlayerNote,
                onPlayerNoteCommit: persistPlayerNote,
                refreshWeeklies: () => loadTabData("weeklies"),
                randomizerCatalog,
                randomizerActions: {
                    isBusy,
                    generateRandomizer: async (payload) => {
                        const result = await postConfigAction(
                            "randomizer_generate",
                            payload,
                        );
                        if (
                            !result ||
                            !result.result ||
                            result.result.ok !== true ||
                            !result.randomizer
                        ) {
                            return null;
                        }
                        return result.randomizer;
                    },
                },
                performanceActions: {
                    isBusy,
                    activeHotkeyPath,
                    beginHotkeyCapture,
                    endHotkeyCapture,
                    hotkeyStringFromEvent,
                    triggerOverlayAction,
                    isHotkeyClearKey,
                    isHotkeyModifierKey,
                },
                performanceDisplayVisible:
                    Boolean(getAtPath(draft, ["performance_show"], false)) ||
                    performanceEditModeEnabled,
                languageManager,
                statsState,
                statsActions: {
                    isBusy,
                    setStatsState,
                    refreshStats: () => refreshStatistics(false, null, true),
                    startSimpleAnalysis,
                    runDetailedAnalysis,
                    pauseDetailedAnalysis,
                    dumpData,
                    deleteParsedData,
                    setDetailedAnalysisAtStart,
                    showReplay,
                    revealReplay,
                    setStatsBool,
                    setStatsText,
                    setStatsNumber,
                    toggleDifficulty,
                    toggleRegion,
                },
                gamesState: {
                    isBusy,
                    selectedReplayFile,
                    setSelectedReplayFile,
                    searchText: gamesSearch,
                    setSearchText: setGamesSearch,
                    totalRows: tabData.games?.totalRows || 0,
                    loadedRows: Array.isArray(tabData.games?.rows)
                        ? tabData.games.rows.length
                        : 0,
                    refresh: () =>
                        loadTabData("games", true, {
                            gamesLimit: gamesLoadLimitRef.current,
                        }),
                    ensureAllRowsLoaded: async () => {
                        const loadedRows = Array.isArray(tabData.games?.rows)
                            ? tabData.games.rows.length
                            : 0;
                        const totalRows = Number(tabData.games?.totalRows) || 0;
                        if (totalRows <= 0 || loadedRows >= totalRows) {
                            return;
                        }
                        await loadTabData("games", true, {
                            gamesLimit: totalRows,
                        });
                    },
                    ensureRowsForPage: async (page, rowsPerPage) => {
                        const safePage = Math.max(1, Number(page) || 1);
                        const safeRowsPerPage = Math.max(
                            1,
                            Number(rowsPerPage) || 20,
                        );
                        const requiredRows = safePage * safeRowsPerPage;
                        const loadedRows = Array.isArray(tabData.games?.rows)
                            ? tabData.games.rows.length
                            : 0;
                        const totalRows = Number(tabData.games?.totalRows) || 0;
                        if (
                            requiredRows <= loadedRows ||
                            (totalRows > 0 && loadedRows >= totalRows)
                        ) {
                            return;
                        }
                        await loadTabData("games", true, {
                            gamesLimit: Math.max(
                                gamesLoadLimitRef.current,
                                requiredRows,
                            ),
                        });
                    },
                    showSelected: () => showSelectedReplay(),
                    moveReplay,
                    showReplay: showReplayByFile,
                    loadChat: loadReplayChat,
                    revealFile: revealReplayByFile,
                },
            })
        );

    return (
        <section id="app-content">
            <div id="app-tab-nav" className="tabs">
                {TABS.map((tab) => (
                    <button
                        key={tab.id}
                        type="button"
                        className={`tab-btn${tab.id === activeTab ? " is-active" : ""}`}
                        data-tab={tab.id}
                        disabled={draft === null}
                        onClick={() => {
                            setActiveTab(tab.id);
                            refreshDataTabOnClick(tab.id);
                        }}
                    >
                        {languageManager.translate(tab.titleId)}
                    </button>
                ))}
            </div>
            <p id="app-status" className="status" data-busy={String(isBusy)}>
                {status}
            </p>
            {tabContent}
            <div
                id="app-footer"
                className={
                    active.id === "settings" ? "footer is-hidden" : "footer"
                }
            >
                <button
                    id="app-save"
                    type="button"
                    className="submit"
                    disabled={!dirty || isBusy || draft === null}
                    onClick={saveSettings}
                >
                    {isBusy
                        ? languageManager.translate("ui_footer_saving")
                        : languageManager.translate("ui_footer_apply_save")}
                </button>
                <button
                    id="app-revert"
                    type="button"
                    className="submit"
                    disabled={!dirty || isBusy || draft === null}
                    onClick={resetSettings}
                >
                    {languageManager.translate("ui_footer_reset")}
                </button>
                <button
                    id="app-reload"
                    type="button"
                    className="submit"
                    disabled={isBusy || draft === null}
                    onClick={loadSettings}
                >
                    {languageManager.translate("ui_footer_reload")}
                </button>
            </div>
        </section>
    );
}

export default SettingsEditor;
