import * as React from "react";
import { Tab, Tabs } from "@mui/material";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Link as RouterLink, useLocation, useNavigate } from "react-router-dom";
import type {
    AppSettings,
    MonitorOption,
    OverlayActionResponse,
    OverlayRandomizerCatalog,
    OverlayScreenshotResultPayload,
    PerformanceVisibilityPayload,
    ReplayChatPayload,
    ReplayVisualPayload,
} from "../../bindings/overlay";

import { createLanguageManager } from "../i18n/languageManager";
import {
    loadReplayChatRequest,
    loadReplayVisualRequest,
    moveReplayRequest,
    postConfigActionRequest,
    postStatsActionRequest,
    showReplayRequest,
} from "./configApi";
import { getAtPath, setAtPath } from "./configValueUtils";
import {
    useConfigTabData,
    type GamesPayload,
    type PlayersPayload,
    type TabDataState,
} from "./hooks/useConfigTabData";
import { useConfigStats } from "./hooks/useConfigStats";
import { useConfigSettings } from "./hooks/useConfigSettings";
import { useHotkeyCapture } from "./hooks/useHotkeyCapture";
import type {
    DisplayValue,
    JsonArray,
    JsonObject,
    JsonValue,
    StatisticsPayload,
} from "./types";
import GamesTab from "./tabs/GamesTab";
import GenericTab from "./tabs/GenericTab";
import PerformanceTab from "./tabs/PerformanceTab";
import PlayersTab from "./tabs/PlayersTab";
import RandomizerTab from "./tabs/RandomizerTab";
import SettingsTab from "./tabs/SettingsTab";
import StatisticsTab from "./tabs/StatisticsTab";
import WeekliesTab from "./tabs/WeekliesTab";
import styles from "./page.module.css";

const { useEffect, useMemo, useRef, useState } = React;

type GamesChatPayload = ReplayChatPayload | null;
type GamesVisualPayload = ReplayVisualPayload | null;
type PathValueUpdater = (path: string[], value: JsonValue) => void;
type SettingsEditorProps = {
    onThemeModeChange: (darkThemeEnabled: boolean) => void;
    appVersion: string;
    isDev: boolean;
};
type LanguageManagerInstance = ReturnType<typeof createLanguageManager>;
type TabId =
    | "settings"
    | "games"
    | "players"
    | "weeklies"
    | "statistics"
    | "randomizer"
    | "performance"
    | "links";
type TabGroup = {
    title: string;
    paths?: string[][];
    links?: Array<[string, string]>;
};
type ConfigTabDefinition = {
    id: TabId;
    titleId: string;
    groups: TabGroup[];
};
type SettingsTabActions = React.ComponentProps<typeof SettingsTab>["actions"];
type RandomizerTabActions = React.ComponentProps<
    typeof RandomizerTab
>["actions"];
type PerformanceTabActions = React.ComponentProps<
    typeof PerformanceTab
>["actions"];
type GamesTabState = React.ComponentProps<typeof GamesTab>["state"];
type PlayersTabState = React.ComponentProps<typeof PlayersTab>["state"];
type ConfigStatsResult = ReturnType<typeof useConfigStats>;
type ExtraState = {
    tabData: TabDataState;
    isDev: boolean;
    isBusy: boolean;
    settingsActions: SettingsTabActions;
    refreshPlayers: () => void;
    playersState: PlayersTabState;
    playerNotes: Record<string, string>;
    onPlayerNoteChange: (handle: string, note: string) => void;
    onPlayerNoteCommit: (handle: string, note: string) => Promise<void>;
    refreshWeeklies: () => void;
    randomizerCatalog: OverlayRandomizerCatalog | null;
    randomizerActions: RandomizerTabActions;
    performanceActions: PerformanceTabActions;
    performanceDisplayVisible: boolean;
    languageManager: LanguageManagerInstance;
    statsState: ConfigStatsResult["statsState"];
    statsActions: ConfigStatsResult["statsActions"];
    gamesState: GamesTabState & {
        showSelected: () => void;
        moveReplay: (delta: number) => Promise<void>;
    };
};
declare global {
    interface Window {
        __scoSetPerformanceVisibility?: (visible: boolean) => void;
    }
}

const TABS: ConfigTabDefinition[] = [
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
                    ["show_charts"],
                    ["hide_nicknames_in_overlay"],
                    ["dark_theme"],
                ],
            },
            {
                title: "Paths",
                paths: [["account_folder"], ["screenshot_folder"]],
            },
            {
                title: "Overlay",
                paths: [["monitor"], ["duration"]],
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

const SCO_PERFORMANCE_VISIBILITY_EVENT = "sco://performance-visibility";
const SCO_OVERLAY_SCREENSHOT_RESULT_EVENT = "sco://overlay-screenshot-result";
const DEFAULT_TAB_ID: TabId = "settings";

function getTabRoute(tabId: TabId): string {
    return `/config/${tabId}`;
}

function isTabId(value: string): value is TabId {
    return TABS.some((tab) => tab.id === value);
}

function getTabIdFromPathname(pathname: string): TabId | null {
    const parts = pathname.split("/").filter((part) => part.length > 0);
    if (parts.length < 2 || parts[0] !== "config") {
        return null;
    }
    const candidate = parts[1];
    return isTabId(candidate) ? candidate : null;
}

function performanceVisibilityFromPayload(
    payload: PerformanceVisibilityPayload | null | undefined,
): boolean | null {
    if (!payload || typeof payload !== "object") {
        return null;
    }
    if (!("visible" in payload)) {
        return null;
    }
    return Boolean(payload.visible);
}

function prettyLabel(value: string): string {
    return value
        .replace(/_/g, " ")
        .replace(/\//g, " / ")
        .replace(/([a-z])([A-Z])/g, "$1 $2")
        .replace(/\b\w/g, (match) => match.toUpperCase());
}

function isSensitivePath(path: string[]): boolean {
    const full = path.join(".").toLowerCase();
    return (
        full.includes("secret") ||
        full.includes("oauth") ||
        full.endsWith("password") ||
        full.endsWith("api_key")
    );
}

function asArrayFromText(
    raw: string,
    templateValue: JsonValue | undefined,
): string[] | number[] {
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

function asTextFromValue(value: DisplayValue | JsonArray): string {
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

function renderNode(
    value: JsonValue | undefined,
    templateValue: JsonValue | undefined,
    path: string[],
    depth: number,
    onChange: PathValueUpdater,
): React.ReactNode {
    const nodeDepthClassName =
        depth <= 0
            ? styles.nodeDepth0
            : depth === 1
              ? styles.nodeDepth1
              : depth === 2
                ? styles.nodeDepth2
                : styles.nodeDepth3;
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
                <label className={styles.field}>
                    <span className={styles.fieldLabel}>{label}</span>
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
                <label
                    className={[styles.field, styles.fieldTextarea]
                        .filter(Boolean)
                        .join(" ")}
                >
                    <span
                        className={styles.fieldLabel}
                    >{`${label} (one row per line)`}</span>
                    <textarea
                        rows={Math.max(
                            3,
                            Array.isArray(value) ? value.length : 3,
                        )}
                        className={[styles.mono, styles.input]
                            .filter(Boolean)
                            .join(" ")}
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
                <label className={styles.field}>
                    <span className={styles.fieldLabel}>{label}</span>
                    <input
                        type="number"
                        step="any"
                        value={Number.isFinite(value) ? value : 0}
                        className={styles.input}
                        onChange={(event) =>
                            onChange(path, Number(event.target.value))
                        }
                    />
                </label>
            );
        }

        return (
            <label className={styles.field}>
                <span className={styles.fieldLabel}>{label}</span>
                <input
                    type={isSensitivePath(path) ? "password" : "text"}
                    value={asTextFromValue(value)}
                    className={styles.input}
                    onChange={(event) => onChange(path, event.target.value)}
                />
            </label>
        );
    }

    if (typeof value === "object") {
        const entries = Object.entries(value);
        return (
            <details
                className={[nodeDepthClassName, styles.card]
                    .filter(Boolean)
                    .join(" ")}
                open
            >
                <summary className={styles.sectionTitle}>{label}</summary>
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

function formatPercent(value: DisplayValue): string {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "0.0%";
    }
    return `${(num * 100).toFixed(1)}%`;
}

function normalizeDate(value: DisplayValue): string {
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

function asTableValue(value: DisplayValue): string {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

function formatPercent0(value: DisplayValue): string {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "-";
    }
    return `${(num * 100).toFixed(0)}%`;
}

function formatPercent1(value: DisplayValue): string {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "-";
    }
    return `${(num * 100).toFixed(1)}%`;
}

function formatNumber(value: DisplayValue): string {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return asTableValue(value);
    }
    return num.toLocaleString("en-US");
}

function formatDurationSeconds(value: DisplayValue): string {
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

function hotkeyStringFromEvent(
    event: React.KeyboardEvent<HTMLInputElement>,
): string {
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

function isHotkeyClearKey(key: string): boolean {
    return (
        key === "Backspace" ||
        key === "Delete" ||
        key === "Escape" ||
        key === "Esc"
    );
}

function isHotkeyModifierKey(key: string): boolean {
    return (
        key === "Control" || key === "Shift" || key === "Alt" || key === "Meta"
    );
}

function renderGamesTab(
    rows: GamesPayload | React.ComponentProps<typeof GamesTab>["rows"],
    state: GamesTabState,
    languageManager: LanguageManagerInstance,
    isDev: boolean,
): React.ReactNode {
    const gameRows = Array.isArray(rows)
        ? rows
        : rows !== null && typeof rows === "object" && "rows" in rows
          ? rows.rows
          : null;
    return (
        <GamesTab
            rows={gameRows}
            state={state}
            isDev={isDev}
            asTableValue={asTableValue}
            formatDurationSeconds={formatDurationSeconds}
            languageManager={languageManager}
        />
    );
}

function renderPlayersTab(
    rows: PlayersPayload | React.ComponentProps<typeof PlayersTab>["rows"],
    state: PlayersTabState,
    languageManager: LanguageManagerInstance,
    playerNotes: React.ComponentProps<typeof PlayersTab>["noteValues"],
    onPlayerNoteChange: (handle: string, note: string) => void,
    onPlayerNoteCommit: (handle: string, note: string) => Promise<void>,
): React.ReactNode {
    const playerRows = Array.isArray(rows)
        ? rows
        : rows !== null && typeof rows === "object" && "rows" in rows
          ? rows.rows
          : null;
    return (
        <PlayersTab
            rows={playerRows}
            state={state}
            noteValues={playerNotes}
            onNoteChange={onPlayerNoteChange}
            onNoteCommit={onPlayerNoteCommit}
            asTableValue={asTableValue}
            formatPercent={formatPercent}
            languageManager={languageManager}
        />
    );
}

function renderWeekliesTab(
    rows: React.ComponentProps<typeof WeekliesTab>["rows"],
    onRefresh: () => void,
    isBusy: boolean,
    languageManager: LanguageManagerInstance,
): React.ReactNode {
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
    statsPayload: StatisticsPayload | null,
    statsState: ConfigStatsResult["statsState"],
    actions: ConfigStatsResult["statsActions"],
    languageManager: LanguageManagerInstance,
): React.ReactNode {
    return (
        <StatisticsTab
            statsPayload={statsPayload}
            statsState={statsState}
            actions={actions}
            languageManager={languageManager}
        />
    );
}

function renderMainSettingsTab(
    draft: React.ComponentProps<typeof SettingsTab>["draft"],
    onChange: PathValueUpdater,
    actions: SettingsTabActions,
    languageManager: LanguageManagerInstance,
): React.ReactNode {
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

function renderRandomizerTab(
    draft: React.ComponentProps<typeof RandomizerTab>["draft"],
    onChange: PathValueUpdater,
    extraState: ExtraState,
    languageManager: LanguageManagerInstance,
): React.ReactNode {
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

function renderPerformanceTab(
    draft: React.ComponentProps<typeof PerformanceTab>["draft"],
    onChange: PathValueUpdater,
    extraState: ExtraState,
): React.ReactNode {
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

function renderTabContent(
    tab: ConfigTabDefinition,
    draft: AppSettings,
    settings: AppSettings | null,
    onChange: PathValueUpdater,
    extraState: ExtraState,
): React.ReactNode {
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
            extraState.isDev,
        );
    }
    if (tab.id === "players") {
        return renderPlayersTab(
            extraState.tabData.players,
            extraState.playersState,
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
            tab={{ title: tab.titleId, groups: tab.groups }}
            draft={draft}
            settings={settings}
            onChange={onChange}
            renderNode={renderNode}
            getAtPath={getAtPath}
        />
    );
}

function SettingsEditor({
    onThemeModeChange,
    appVersion,
    isDev,
}: SettingsEditorProps): React.ReactNode {
    const location = useLocation();
    const navigate = useNavigate();
    const [isBusy, setIsBusy] = useState(false);
    const [selectedReplayFile, setSelectedReplayFile] = useState("");
    const [gamesSearch, setGamesSearch] = useState("");
    const [performanceEditModeEnabled, setPerformanceEditModeEnabled] =
        useState(false);
    const {
        applyRuntimeSettings,
        cancelPendingLiveApply,
        dirty,
        draft,
        draftRef,
        loadSettings,
        monitorCatalog,
        randomizerCatalog,
        replaceDraft,
        resetSettings,
        safeStatus,
        saveSettings,
        setDraft,
        setSettings,
        settings,
        settingsMutationRef,
        setStatus,
        status,
        updateField,
    } = useConfigSettings({ onThemeModeChange, setIsBusy });
    const {
        gamesPageRequestRef,
        loadTabData,
        playersPageRequestRef,
        setTabData,
        tabData,
    } = useConfigTabData({ setIsBusy, safeStatus });
    const { activeHotkeyPath, beginHotkeyCapture, endHotkeyCapture } =
        useHotkeyCapture({
            settingsMutationRef,
        });
    const activeTab = useMemo<TabId>(
        () => getTabIdFromPathname(location.pathname) ?? DEFAULT_TAB_ID,
        [location.pathname],
    );
    const { statsActions, statsState } = useConfigStats({
        activeTab,
        draft,
        isBusy,
        safeStatus,
        setIsBusy,
        setTabData,
        tabData,
    });
    const languageManager = useMemo(
        () =>
            createLanguageManager(
                String(draft?.language || settings?.language || "en"),
            ),
        [draft, settings],
    );

    useEffect(() => {
        if (getTabIdFromPathname(location.pathname) !== null) {
            return;
        }
        navigate(getTabRoute(DEFAULT_TAB_ID), { replace: true });
    }, [location.pathname, navigate]);

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

    function applyPerformanceVisibilityState(visible: boolean): void {
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

    useEffect(() => {
        loadSettings();
    }, []);

    useEffect(() => {
        let isMounted = true;
        let unlisten = null;
        (async () => {
            if (!isMounted) {
                return;
            }

            try {
                unlisten = await listen<PerformanceVisibilityPayload>(
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

    useEffect(() => {
        if (activeTab === "weeklies" && tabData.weeklies === null) {
            loadTabData("weeklies");
        }
    }, [activeTab, tabData.weeklies]);

    async function postAction<T extends { message?: string }>(
        request: () => Promise<T>,
    ): Promise<T | null> {
        setIsBusy(true);
        try {
            const result = await request();
            safeStatus(result.message || "Action completed");
            return result;
        } catch (error) {
            safeStatus(`Action failed: ${error.message}`);
            return null;
        } finally {
            setIsBusy(false);
        }
    }

    function normalizePlayerNoteKey(value: DisplayValue): string {
        return asTableValue(value).trim().toLowerCase();
    }

    function patchedPlayerNotes(
        currentSettings: AppSettings,
        handle: string,
        noteValue: string,
    ): Record<string, string> | undefined {
        const currentNotesValue = getAtPath(currentSettings, ["player_notes"]);
        const currentNotes: Record<string, string> =
            currentNotesValue &&
            typeof currentNotesValue === "object" &&
            !Array.isArray(currentNotesValue)
                ? Object.fromEntries(
                      Object.entries(currentNotesValue).map(([key, value]) => [
                          key,
                          typeof value === "string" ? value : String(value),
                      ]),
                  )
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

    function updatePlayerNote(handle: string, noteValue: string): void {
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

    async function persistPlayerNote(
        handle: string,
        noteValue: string,
    ): Promise<void> {
        try {
            setIsBusy(true);
            const payload = await postConfigActionRequest("set_player_note", {
                player: handle,
                note: noteValue,
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

    async function setLatestFirstWinBonusTime(value: string): Promise<void> {
        try {
            setIsBusy(true);
            const payload = await postConfigActionRequest(
                "set_latest_today_win_bonus_time",
                { time: value },
            );
            setSettings((current) => {
                if (current === null) {
                    return current;
                }
                return setAtPath(
                    current,
                    ["latest_today_win_bonus_time"],
                    value,
                );
            });
            setDraft((current) => {
                if (current === null) {
                    return current;
                }
                const nextDraft = setAtPath(
                    current,
                    ["latest_today_win_bonus_time"],
                    value,
                );
                draftRef.current = nextDraft;
                return nextDraft;
            });
            safeStatus(payload.message || "Latest first win bonus time saved.");
        } catch (error) {
            safeStatus(`Failed to save first win bonus time: ${error.message}`);
        } finally {
            setIsBusy(false);
        }
    }

    async function showSelectedReplay() {
        if (!selectedReplayFile) {
            setStatus("Select a replay first");
            return;
        }
        const result = await postAction(() =>
            showReplayRequest(selectedReplayFile),
        );
        if (result) {
            setStatus("Replay sent to overlay");
            await loadTabData("games");
        }
    }

    async function showReplayByFile(file: string): Promise<void> {
        if (!file) {
            return;
        }
        setSelectedReplayFile(file);
        const result = await postAction(() => showReplayRequest(file));
        if (result) {
            setStatus("Replay sent to overlay");
        }
    }

    async function loadReplayChat(file: string): Promise<GamesChatPayload> {
        if (!file) {
            return null;
        }
        const result = await loadReplayChatRequest(file);
        return (result.chat as GamesChatPayload) || null;
    }

    async function loadReplayVisual(file: string): Promise<GamesVisualPayload> {
        if (!file) {
            return null;
        }
        const result = await loadReplayVisualRequest(file);
        return (result.visual as GamesVisualPayload) || null;
    }

    async function revealReplayByFile(file: string): Promise<void> {
        if (!file) {
            return;
        }
        await postAction(() => postStatsActionRequest("reveal_file", { file }));
    }

    async function moveReplay(delta: number): Promise<void> {
        const result = await postAction(() => moveReplayRequest(delta));
        if (result) {
            await loadTabData("games", false, {
                gamesRequest: gamesPageRequestRef.current,
            });
        }
    }

    async function postConfigAction(
        action: string,
        payload: JsonObject = {},
    ): Promise<OverlayActionResponse | null> {
        return postAction(() => postConfigActionRequest(action, payload));
    }

    async function promptPath(path: string[], title: string): Promise<void> {
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

    async function triggerOverlayAction(actionName: string): Promise<void> {
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

    async function openFolderPath(path: string): Promise<true | null> {
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

    const active = TABS.find((tab) => tab.id === activeTab) || TABS[0];
    const tabContent =
        draft === null ? (
            <section className={styles.tabContent}>
                <div
                    className={[styles.card, styles.group]
                        .filter(Boolean)
                        .join(" ")}
                >
                    <p>{status}</p>
                </div>
            </section>
        ) : (
            renderTabContent(active, draft, settings, updateField, {
                tabData,
                isDev,
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
                    runDetailedAnalysis: statsActions.runDetailedAnalysis,
                    startSimpleAnalysis: statsActions.startSimpleAnalysis,
                    stopDetailedAnalysis: statsActions.stopDetailedAnalysis,
                    deleteParsedData: statsActions.deleteParsedData,
                    applyMainSettings,
                    resetMainSettings,
                    setLatestFirstWinBonusTime,
                    monitorOptions: monitorCatalog,
                    isHotkeyClearKey,
                    isHotkeyModifierKey,
                    analysisRunning: Boolean(
                        tabData.statistics?.analysis_running,
                    ),
                    analysisRunningMode:
                        typeof tabData.statistics?.analysis_running_mode ===
                        "string"
                            ? tabData.statistics.analysis_running_mode
                            : null,
                    simpleAnalysisStatus: String(
                        tabData.statistics?.simple_analysis_status || "",
                    ),
                    detailedAnalysisStatus: String(
                        tabData.statistics?.detailed_analysis_status || "",
                    ),
                    analysisMessage: String(tabData.statistics?.message || ""),
                    analysisScanProgress:
                        tabData.statistics?.scan_progress &&
                        typeof tabData.statistics.scan_progress === "object" &&
                        !Array.isArray(tabData.statistics.scan_progress)
                            ? (tabData.statistics.scan_progress as Record<
                                  string,
                                  JsonValue
                              >)
                            : null,
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
                        ? (draft.player_notes as Record<string, string>)
                        : ({} as Record<string, string>),
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

                        const randomizerResult = result.randomizer;
                        if (randomizerResult.kind === "mutator") {
                            return {
                                kind: "mutator" as const,
                                mutators: Array.isArray(
                                    randomizerResult.mutators,
                                )
                                    ? randomizerResult.mutators.map(
                                          (mutator) => ({
                                              id: String(mutator.id),
                                              name: {
                                                  en: String(
                                                      mutator.name?.en || "",
                                                  ),
                                                  ko: String(
                                                      mutator.name?.ko || "",
                                                  ),
                                              },
                                              iconName: String(
                                                  mutator.iconName || "",
                                              ),
                                              description: {
                                                  en: String(
                                                      mutator.description?.en ||
                                                          "",
                                                  ),
                                                  ko: String(
                                                      mutator.description?.ko ||
                                                          "",
                                                  ),
                                              },
                                              points: Number(
                                                  mutator.points ?? 0,
                                              ),
                                          }),
                                      )
                                    : [],
                                mutator_total_points: Number(
                                    randomizerResult.mutator_total_points || 0,
                                ),
                                mutator_count: Number(
                                    randomizerResult.mutator_count || 0,
                                ),
                                brutal_plus:
                                    randomizerResult.brutal_plus === null ||
                                    randomizerResult.brutal_plus === undefined
                                        ? null
                                        : Number(randomizerResult.brutal_plus),
                            };
                        }

                        return {
                            kind: "commander" as const,
                            commander: String(randomizerResult.commander || ""),
                            prestige: Number(randomizerResult.prestige || 0),
                            mastery_indices: Array.isArray(
                                randomizerResult.mastery_indices,
                            )
                                ? randomizerResult.mastery_indices.map(
                                      (value) =>
                                          value === null ? null : Number(value),
                                  )
                                : [],
                            map_race: String(randomizerResult.map_race || ""),
                        };
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
                    Boolean(getAtPath(draft, ["performance_show"])) ||
                    performanceEditModeEnabled,
                languageManager,
                statsState,
                statsActions,
                gamesState: {
                    isBusy,
                    selectedReplayFile,
                    setSelectedReplayFile,
                    searchText: gamesSearch,
                    setSearchText: setGamesSearch,
                    totalRows: tabData.games?.totalRows || 0,
                    refresh: () =>
                        loadTabData("games", true, {
                            gamesRequest: gamesPageRequestRef.current,
                        }),
                    loadPage: async (request) => {
                        await loadTabData("games", true, {
                            gamesRequest: request,
                        });
                    },
                    showSelected: () => showSelectedReplay(),
                    moveReplay,
                    showReplay: showReplayByFile,
                    loadChat: loadReplayChat,
                    loadVisual: loadReplayVisual,
                    revealFile: revealReplayByFile,
                },
                playersState: {
                    isBusy,
                    totalRows: tabData.players?.totalRows || 0,
                    refresh: () =>
                        loadTabData("players", true, {
                            playersRequest: playersPageRequestRef.current,
                        }),
                    loadPage: async (request) => {
                        await loadTabData("players", true, {
                            playersRequest: request,
                        });
                    },
                },
            })
        );

    return (
        <section id="app-content">
            <div className={styles.configHeader}>
                <h1>
                    SC2 Coop Info v{appVersion}
                    {isDev ? " Dev" : ""}
                </h1>
                <p
                    id="app-status"
                    className={styles.status}
                    data-busy={String(isBusy)}
                >
                    {status}
                </p>
            </div>
            <Tabs
                id="app-tab-nav"
                className={styles.tabs}
                value={activeTab}
                variant="scrollable"
                scrollButtons="auto"
                allowScrollButtonsMobile
            >
                {TABS.map((tab) => (
                    <Tab
                        key={tab.id}
                        value={tab.id}
                        className={[
                            styles.tabBtn,
                            tab.id === activeTab ? styles.isActive : "",
                        ]
                            .filter(Boolean)
                            .join(" ")}
                        label={languageManager.translate(tab.titleId)}
                        component={RouterLink}
                        to={getTabRoute(tab.id)}
                        sx={{ marginRight: "7px" }}
                        disabled={draft === null}
                    />
                ))}
            </Tabs>
            {tabContent}
            <div
                id="app-footer"
                className={
                    active.id === "settings"
                        ? [styles.footer, styles.isHidden]
                              .filter(Boolean)
                              .join(" ")
                        : styles.footer
                }
            >
                <button
                    id="app-save"
                    type="button"
                    className={[styles.submit, styles.buttonNormal]
                        .filter(Boolean)
                        .join(" ")}
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
                    className={[styles.submit, styles.buttonNormal]
                        .filter(Boolean)
                        .join(" ")}
                    disabled={!dirty || isBusy || draft === null}
                    onClick={resetSettings}
                >
                    {languageManager.translate("ui_footer_reset")}
                </button>
                <button
                    id="app-reload"
                    type="button"
                    className={[styles.submit, styles.buttonNormal]
                        .filter(Boolean)
                        .join(" ")}
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
