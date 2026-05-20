import { useEffect, useRef, useState, type MutableRefObject } from "react";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import FirstWinBonusTimerMode from "../overlay/component/FirstWinBonusTimerMode";
import PlayerStatMode from "../overlay/component/PlayerStatMode";
import { createLanguageManager } from "../i18n/languageManager";
import styles from "../overlay/main.module.css";
import type {
    ConfigPayload,
    FirstWinBonusTimerPayload,
    OverlayInitColorsDurationPayload,
    OverlayLanguagePreviewPayload,
    OverlayPlayerStatsPayload,
} from "../../bindings/overlay";

const playerStatsHideMs = 12000;
const OVERLAY_LANGUAGE_PREVIEW_EVENT = "sco://overlay-language-preview";
const OVERLAY_SHOW_HIDE_PLAYER_STATS_EVENT =
    "sco://overlay-show-hide-player-stats";
const OVERLAY_PLAYER_STATS_EVENT = "sco://overlay-player-stats";
const OVERLAY_INIT_COLORS_DURATION_EVENT = "sco://overlay-init-colors-duration";
const OVERLAY_FIRST_WIN_BONUS_TIMER_EVENT =
    "sco://overlay-first-win-bonus-timer";

enum DisplayMode {
    None,
    PlayerStats,
    FirstWinBonusTimer,
}

type Sc2OverlayEventName =
    | typeof OVERLAY_LANGUAGE_PREVIEW_EVENT
    | typeof OVERLAY_SHOW_HIDE_PLAYER_STATS_EVENT
    | typeof OVERLAY_PLAYER_STATS_EVENT
    | typeof OVERLAY_INIT_COLORS_DURATION_EVENT
    | typeof OVERLAY_FIRST_WIN_BONUS_TIMER_EVENT;

type TimeoutHandle = number;
type DisplayStatus = {
    mode: DisplayMode;
    immediate: boolean;
};
type ConfigRequestPayload = {
    method: "GET";
    path: "/config";
};

function clearTimerRef(timerRef: MutableRefObject<TimeoutHandle | null>): void {
    if (timerRef.current == null) {
        return;
    }

    window.clearTimeout(timerRef.current);
    timerRef.current = null;
}

function createUnlistenMap(): Record<Sc2OverlayEventName, (() => void) | null> {
    return {
        [OVERLAY_LANGUAGE_PREVIEW_EVENT]: null,
        [OVERLAY_SHOW_HIDE_PLAYER_STATS_EVENT]: null,
        [OVERLAY_PLAYER_STATS_EVENT]: null,
        [OVERLAY_INIT_COLORS_DURATION_EVENT]: null,
        [OVERLAY_FIRST_WIN_BONUS_TIMER_EVENT]: null,
    };
}

async function loadOverlayConfig(): Promise<ConfigPayload> {
    try {
        return await invoke<ConfigPayload>("config_get");
    } catch {
        const request: ConfigRequestPayload = {
            method: "GET",
            path: "/config",
        };
        return await invoke<ConfigPayload>("config_request", request);
    }
}

export default function Sc2OverlayPage() {
    const runtimeStartedRef = useRef<boolean>(false);
    const playerStatsHideTimerRef = useRef<TimeoutHandle | null>(null);
    const firstWinBonusTimerPayloadRef =
        useRef<FirstWinBonusTimerPayload | null>(null);
    const unlistenMapRef =
        useRef<Record<Sc2OverlayEventName, (() => void) | null>>(
            createUnlistenMap(),
        );
    const [language, setLanguage] = useState<string>("en");
    const [overlayLanguageManager] = useState(() =>
        createLanguageManager(language),
    );
    const [displayMode, setDisplayMode] = useState<DisplayStatus>({
        mode: DisplayMode.None,
        immediate: true,
    });
    const [playerStatPayload, setPlayerStatPayload] =
        useState<OverlayPlayerStatsPayload | null>(null);
    const [firstWinBonusTimerPayload, setFirstWinBonusTimerPayload] =
        useState<FirstWinBonusTimerPayload | null>(null);

    function applyOverlayLanguage(nextLanguage: string): void {
        setLanguage(nextLanguage);
        overlayLanguageManager.setLanguage(nextLanguage);
    }

    function languagePreviewEventHandler({
        payload,
    }: {
        payload: OverlayLanguagePreviewPayload;
    }): void {
        applyOverlayLanguage(payload.language);
    }

    function initColorsDurationEventHandler({
        payload,
    }: {
        payload: OverlayInitColorsDurationPayload;
    }): void {
        applyOverlayLanguage(payload.language);
    }

    function togglePlayerStatsDisplay(
        payload: OverlayPlayerStatsPayload,
        immediate = true,
    ): void {
        setDisplayMode((previousDisplayMode) => {
            const showingPlayerStats =
                previousDisplayMode.mode === DisplayMode.PlayerStats;

            setPlayerStatPayload(showingPlayerStats ? null : payload);

            return {
                mode: showingPlayerStats
                    ? DisplayMode.None
                    : DisplayMode.PlayerStats,
                immediate,
            };
        });
    }

    function togglePlayerStatsEventHandler({
        payload,
    }: {
        payload: OverlayPlayerStatsPayload;
    }): void {
        togglePlayerStatsDisplay(payload, true);
    }

    function playerStatsOnGameStartEventHandler({
        payload,
    }: {
        payload: OverlayPlayerStatsPayload;
    }): void {
        setPlayerStatPayload(payload);
        setDisplayMode({
            mode: DisplayMode.PlayerStats,
            immediate: true,
        });
    }

    function firstWinBonusTimerEventHandler({
        payload,
    }: {
        payload: FirstWinBonusTimerPayload;
    }): void {
        firstWinBonusTimerPayloadRef.current = payload.visible ? payload : null;
        setFirstWinBonusTimerPayload(payload);
        setDisplayMode((previousDisplayMode) => {
            if (previousDisplayMode.mode === DisplayMode.PlayerStats) {
                return previousDisplayMode;
            }

            if (payload.visible) {
                return {
                    mode: DisplayMode.FirstWinBonusTimer,
                    immediate: false,
                };
            }

            if (previousDisplayMode.mode === DisplayMode.FirstWinBonusTimer) {
                return {
                    mode: DisplayMode.None,
                    immediate: false,
                };
            }

            return previousDisplayMode;
        });
    }

    async function initializeOverlay(): Promise<void> {
        if (runtimeStartedRef.current) {
            return;
        }

        runtimeStartedRef.current = true;

        try {
            const response = await loadOverlayConfig();
            applyOverlayLanguage(response.active_settings.language);
        } catch (error) {
            console.warn("Failed to load SC2 overlay config", error);
        }

        try {
            await Promise.all([
                listen<OverlayLanguagePreviewPayload>(
                    OVERLAY_LANGUAGE_PREVIEW_EVENT,
                    languagePreviewEventHandler,
                ).then((unlisten) => {
                    unlistenMapRef.current[OVERLAY_LANGUAGE_PREVIEW_EVENT]?.();
                    unlistenMapRef.current[OVERLAY_LANGUAGE_PREVIEW_EVENT] =
                        unlisten;
                }),
                listen<OverlayPlayerStatsPayload>(
                    OVERLAY_SHOW_HIDE_PLAYER_STATS_EVENT,
                    togglePlayerStatsEventHandler,
                ).then((unlisten) => {
                    unlistenMapRef.current[
                        OVERLAY_SHOW_HIDE_PLAYER_STATS_EVENT
                    ]?.();
                    unlistenMapRef.current[
                        OVERLAY_SHOW_HIDE_PLAYER_STATS_EVENT
                    ] = unlisten;
                }),
                listen<OverlayPlayerStatsPayload>(
                    OVERLAY_PLAYER_STATS_EVENT,
                    playerStatsOnGameStartEventHandler,
                ).then((unlisten) => {
                    unlistenMapRef.current[OVERLAY_PLAYER_STATS_EVENT]?.();
                    unlistenMapRef.current[OVERLAY_PLAYER_STATS_EVENT] =
                        unlisten;
                }),
                listen<OverlayInitColorsDurationPayload>(
                    OVERLAY_INIT_COLORS_DURATION_EVENT,
                    initColorsDurationEventHandler,
                ).then((unlisten) => {
                    unlistenMapRef.current[
                        OVERLAY_INIT_COLORS_DURATION_EVENT
                    ]?.();
                    unlistenMapRef.current[OVERLAY_INIT_COLORS_DURATION_EVENT] =
                        unlisten;
                }),
                listen<FirstWinBonusTimerPayload>(
                    OVERLAY_FIRST_WIN_BONUS_TIMER_EVENT,
                    firstWinBonusTimerEventHandler,
                ).then((unlisten) => {
                    unlistenMapRef.current[
                        OVERLAY_FIRST_WIN_BONUS_TIMER_EVENT
                    ]?.();
                    unlistenMapRef.current[
                        OVERLAY_FIRST_WIN_BONUS_TIMER_EVENT
                    ] = unlisten;
                }),
            ]);
        } catch {
            console.warn("Tauri SC2 overlay events are unavailable.");
        }
    }

    function destroyOverlayRuntime(): void {
        runtimeStartedRef.current = false;

        for (const eventName of Object.keys(
            unlistenMapRef.current,
        ) as Sc2OverlayEventName[]) {
            const unlisten = unlistenMapRef.current[eventName];
            unlisten?.();
            unlistenMapRef.current[eventName] = null;
        }

        clearTimerRef(playerStatsHideTimerRef);
        firstWinBonusTimerPayloadRef.current = null;
    }

    useEffect(() => {
        const root = document.documentElement;
        const body = document.body;
        const previousRootStyle = {
            background: root.style.background,
            height: root.style.height,
            width: root.style.width,
        };
        const previousBodyStyle = {
            background: body.style.background,
            height: body.style.height,
            margin: body.style.margin,
            overflow: body.style.overflow,
            padding: body.style.padding,
            width: body.style.width,
        };

        root.style.background = "transparent";
        root.style.height = "100%";
        root.style.width = "100%";
        body.style.background = "transparent";
        body.style.height = "100%";
        body.style.margin = "0";
        body.style.overflow = "hidden";
        body.style.padding = "0";
        body.style.width = "100%";

        void initializeOverlay();

        return () => {
            destroyOverlayRuntime();
            root.style.background = previousRootStyle.background;
            root.style.height = previousRootStyle.height;
            root.style.width = previousRootStyle.width;
            body.style.background = previousBodyStyle.background;
            body.style.height = previousBodyStyle.height;
            body.style.margin = previousBodyStyle.margin;
            body.style.overflow = previousBodyStyle.overflow;
            body.style.padding = previousBodyStyle.padding;
            body.style.width = previousBodyStyle.width;
        };
    }, []);

    useEffect(() => {
        if (
            displayMode.mode !== DisplayMode.PlayerStats ||
            playerStatPayload == null
        ) {
            clearTimerRef(playerStatsHideTimerRef);
            return;
        }

        playerStatsHideTimerRef.current = window.setTimeout(() => {
            setPlayerStatPayload(null);
            setDisplayMode({
                mode:
                    firstWinBonusTimerPayloadRef.current == null
                        ? DisplayMode.None
                        : DisplayMode.FirstWinBonusTimer,
                immediate: false,
            });
            playerStatsHideTimerRef.current = null;
        }, playerStatsHideMs);

        return () => {
            clearTimerRef(playerStatsHideTimerRef);
        };
    }, [displayMode.mode, playerStatPayload]);

    return (
        <div className={styles.overlayPageRoot}>
            <PlayerStatMode
                payload={playerStatPayload}
                visible={displayMode.mode === DisplayMode.PlayerStats}
                immediate={displayMode.immediate}
                language={language}
                overlayLanguageManager={overlayLanguageManager}
            />
            <FirstWinBonusTimerMode
                payload={firstWinBonusTimerPayload}
                visible={displayMode.mode === DisplayMode.FirstWinBonusTimer}
                immediate={displayMode.immediate}
                overlayLanguageManager={overlayLanguageManager}
            />
        </div>
    );
}
