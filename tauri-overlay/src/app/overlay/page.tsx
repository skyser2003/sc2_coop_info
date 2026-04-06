import { useInsertionEffect, useEffect, useState } from "react";

import { ReplayChartVisible } from "./component/GameStatChart";
import GameStatMode from "./component/GameStatMode";
import PlayerStatMode from "./component/PlayerStatMode";
import { createLanguageManager } from "../i18n/languageManager";
import { emit, listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { snapdom } from "@zumer/snapdom";

import {
    destroy_overlay_charts,
    reset_overlay_chart_pixel_ratio,
    set_overlay_chart_pixel_ratio,
} from "./charts";
import type { DisplayValue } from "../config/types";
import type {
    ConfigPayload,
    OverlayColorPreviewPayload,
    OverlayInitColorsDurationPayload,
    OverlayLanguagePreviewPayload,
    OverlayPlayerInfoPayload,
    OverlayReplayPayload,
    OverlayScreenshotRequestPayload,
    OverlayScreenshotResultPayload,
} from "../../bindings/overlay";

const OVERLAY_STYLE_PATHS = ["/overlay/main.css"];
const overlayHideFadeMs = 1000;
const playerStatsHideMs = 12000;
const gameStatsHideMs = 60000;

enum DisplayMode {
    None,
    PlayerStats,
    GameStats,
}

type OverlayEventName =
    | typeof OVERLAY_COLOR_PREVIEW_EVENT
    | typeof OVERLAY_LANGUAGE_PREVIEW_EVENT
    | typeof OVERLAY_REPLAY_PAYLOAD_EVENT
    | typeof OVERLAY_SHOW_HIDE_PLAYER_WINRATE_EVENT
    | typeof OVERLAY_PLAYER_WINRATE_EVENT
    | typeof OVERLAY_INIT_COLORS_DURATION_EVENT
    | typeof OVERLAY_SHOWSTATS_EVENT
    | typeof OVERLAY_HIDESTATS_EVENT
    | typeof OVERLAY_SHOWHIDE_EVENT
    | typeof OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT
    | typeof OVERLAY_SCREENSHOT_REQUEST_EVENT;

type LocalizableValue = string | number | boolean | null | undefined;
type OverlayPrestigeNameCatalog = Record<
    string,
    { en: string[]; ko: string[] }
>;

function formatOverlayScreenshotError(error: DisplayValue | Error): string {
    if (error instanceof Error) {
        return error.message;
    }
    if (typeof error === "string" && error.trim() !== "") {
        return error;
    }
    return `Failed to capture overlay screenshot: ${String(error)}`;
}

function waitForAnimationFrame(): Promise<void> {
    return new Promise((resolve) => {
        window.requestAnimationFrame(() => resolve());
    });
}

function ensureStyleLoaded(href: string): void {
    const selector = `link[data-overlay-style="${href}"]`;
    const existing = document.querySelector(selector) as HTMLLinkElement | null;
    if (existing) {
        return;
    }

    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = href;
    link.dataset.overlayStyle = href;
    document.head.appendChild(link);
}

function ensureOverlayAssetsLoaded(): void {
    for (const href of OVERLAY_STYLE_PATHS) {
        ensureStyleLoaded(href);
    }
}

function setOverlayBackgroundVisible(
    visible: boolean,
    immediate = false,
): void {
    const bg = document.getElementById("bgdiv");
    const ibg = document.getElementById("ibgdiv");

    if (bg == null) {
        return;
    }

    bg.style.transition = immediate ? "opacity 0s" : "";
    bg.style.display = visible ? "block" : "none";
    bg.style.opacity = visible ? "1" : "0";

    if (ibg != null) {
        ibg.style.display = visible ? "block" : "none";
        ibg.style.opacity = visible ? "1" : "0";
    }
}

function reportOverlayReplayDataState(active: boolean): void {
    void (async function () {
        try {
            await invoke("config_action", {
                action: "overlay_replay_data_state",
                payload: {
                    active,
                },
            });
        } catch (error) {
            console.warn("Failed to report overlay replay data state", error);
        }
    })();
}

const OVERLAY_COLOR_PREVIEW_EVENT = "sco://overlay-color-preview";
const OVERLAY_LANGUAGE_PREVIEW_EVENT = "sco://overlay-language-preview";
const OVERLAY_REPLAY_PAYLOAD_EVENT = "sco://overlay-replay-payload";
const OVERLAY_SHOW_HIDE_PLAYER_WINRATE_EVENT =
    "sco://overlay-show-hide-player-winrate";
const OVERLAY_PLAYER_WINRATE_EVENT = "sco://overlay-player-winrate";
const OVERLAY_INIT_COLORS_DURATION_EVENT = "sco://overlay-init-colors-duration";
const OVERLAY_SHOWSTATS_EVENT = "sco://overlay-showstats";
const OVERLAY_HIDESTATS_EVENT = "sco://overlay-hidestats";
const OVERLAY_SHOWHIDE_EVENT = "sco://overlay-showhide";
const OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT =
    "sco://overlay-set-show-charts-from-config";
const OVERLAY_SCREENSHOT_REQUEST_EVENT = "sco://overlay-screenshot-request";
const OVERLAY_SCREENSHOT_RESULT_EVENT = "sco://overlay-screenshot-result";

const tauriUnlistens: Record<OverlayEventName, (() => void) | null> = {
    [OVERLAY_COLOR_PREVIEW_EVENT]: null,
    [OVERLAY_LANGUAGE_PREVIEW_EVENT]: null,
    [OVERLAY_REPLAY_PAYLOAD_EVENT]: null,
    [OVERLAY_SHOW_HIDE_PLAYER_WINRATE_EVENT]: null,
    [OVERLAY_PLAYER_WINRATE_EVENT]: null,
    [OVERLAY_INIT_COLORS_DURATION_EVENT]: null,
    [OVERLAY_SHOWSTATS_EVENT]: null,
    [OVERLAY_HIDESTATS_EVENT]: null,
    [OVERLAY_SHOWHIDE_EVENT]: null,
    [OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT]: null,
    [OVERLAY_SCREENSHOT_REQUEST_EVENT]: null,
};

interface DisplayStatus {
    mode: DisplayMode;
    immediate: boolean;
}

interface DisplayTransitionOptions {
    immediate?: boolean;
    playerPayload?: OverlayPlayerInfoPayload | null;
}

export default function OverlayPage() {
    const [overlayRuntimeStarted, setOverlayRuntimeStarted] =
        useState<boolean>(false);
    const [displayMode, setDisplayMode] = useState<DisplayStatus>({
        mode: DisplayMode.None,
        immediate: true,
    });
    const [language, setLanguage] = useState<string>("en");
    const [overlayLanguageManager] = useState(() =>
        createLanguageManager(language),
    );
    const [overlayHideTimer, setOverlayHideTimer] =
        useState<NodeJS.Timeout | null>(null);
    const [p1Color, setP1Color] = useState<string>("#0080F8");
    const [p2Color, setP2Color] = useState<string>("#00D532");
    const [amonColor, setAmonColor] = useState<string>("red");
    const [masteryColor, setMasteryColor] = useState<string>("#FFDC87");
    const [gameStatPayload, setGameStatPayload] =
        useState<OverlayReplayPayload | null>(null);
    const [replayDisplayClearTimer, setReplayDisplayClearTimer] =
        useState<NodeJS.Timeout | null>(null);
    const [replayExpiryTimer, setReplayExpiryTimer] =
        useState<NodeJS.Timeout | null>(null);
    const [overlayPrestigeNameCatalog, setOverlayPrestigeNameCatalog] =
        useState<OverlayPrestigeNameCatalog>({});
    const [chartVisibility, setChartVisibility] = useState<ReplayChartVisible>({
        visible: true,
        immediate: false,
    });
    const [chartVisibleFromConfig, setChartVisibleFromConfig] =
        useState<boolean>(true);
    const [showSessionStats, setShowSessionStats] = useState<boolean>(false);
    const [hideNicknamesInOverlay, setHideNicknamesInOverlay] =
        useState<boolean>(false);
    const [sessionVictoryCount, setSessionVictoryCount] = useState<number>(0);
    const [sessionDefeatCount, setSessionDefeatCount] = useState<number>(0);
    const [playerStatPayload, setPlayerStatPayload] =
        useState<OverlayPlayerInfoPayload | null>(null);

    function overlayText(id: string): string {
        return overlayLanguageManager.translate(id);
    }

    function overlayLocalize(value: LocalizableValue): string {
        return overlayLanguageManager.localize(value);
    }

    function overlayEnglish(value: LocalizableValue): string {
        return overlayLanguageManager.englishLabel(value);
    }

    async function loadOverlayPrestigeNameCatalog(): Promise<void> {
        try {
            const response = await invoke<ConfigPayload>("config_get");
            setOverlayPrestigeNameCatalog(
                response.randomizer_catalog.prestige_names,
            );
        } catch (error) {
            console.warn("Failed to load overlay prestige catalog", error);
        }
    }

    function applyOverlayLanguage(nextLanguage: string): void {
        setLanguage(nextLanguage);
        overlayLanguageManager.setLanguage(nextLanguage);
        void loadOverlayPrestigeNameCatalog();

        const noData = document.getElementById("nodata");
        if (noData != null) {
            noData.textContent = overlayText("ui_overlay_no_data");
        }

        const bestTime = document.getElementById("record");
        if (bestTime != null) {
            bestTime.textContent = overlayText("ui_overlay_best_time");
        }
    }

    function setColors(
        localP1Color: string | null,
        localP2Color: string | null,
        localAmonColor: string | null,
        localMasteryColor: string | null,
    ): void {
        if (localP1Color != null) {
            setP1Color(localP1Color);
        }
        if (localP2Color != null) {
            setP2Color(localP2Color);
        }
        if (localAmonColor != null) {
            setAmonColor(localAmonColor);
        }
        if (localMasteryColor != null) {
            setMasteryColor(localMasteryColor);
        }
    }

    function cancelOverlayHideTimer(): void {
        if (overlayHideTimer != null) {
            clearTimeout(overlayHideTimer);
            setOverlayHideTimer(null);
        }
    }

    function cancelReplayDisplayClearTimer(): void {
        if (replayDisplayClearTimer != null) {
            clearTimeout(replayDisplayClearTimer);
            setReplayDisplayClearTimer(null);
        }
    }

    function cancelReplayExpiryTimer(): void {
        if (replayExpiryTimer != null) {
            clearTimeout(replayExpiryTimer);
            setReplayExpiryTimer(null);
        }
    }

    function requestDisplayTransition(
        mode: DisplayMode,
        options?: DisplayTransitionOptions,
    ): void {
        const immediate = options?.immediate ?? false;
        const nextPlayerPayload = options?.playerPayload ?? null;

        setPlayerStatPayload(
            mode === DisplayMode.PlayerStats ? nextPlayerPayload : null,
        );
        setDisplayMode({
            mode,
            immediate,
        });
    }

    function toggleGameStatsDisplay(): void {
        setPlayerStatPayload(null);
        setDisplayMode((previousDisplayMode) => ({
            mode:
                previousDisplayMode.mode === DisplayMode.None
                    ? DisplayMode.GameStats
                    : DisplayMode.None,
            immediate: false,
        }));
    }

    function togglePlayerStatsDisplay(
        payload: OverlayPlayerInfoPayload,
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

    function hideStatsPanel(immediate = false): void {
        const stats = document.getElementById("stats");
        if (stats == null) {
            return;
        }

        cancelOverlayHideTimer();

        if (immediate) {
            stats.style.opacity = "0";
            stats.style.right = "-50.5vh";
            stats.style.display = "none";
            return;
        }

        stats.style.opacity = "0";

        const hideTimer = setTimeout(() => {
            const statsElement = document.getElementById("stats");

            if (statsElement != null) {
                statsElement.style.right = "-50.5vh";
                statsElement.style.display = "none";
            }

            setOverlayHideTimer(null);
        }, overlayHideFadeMs);

        setOverlayHideTimer(hideTimer);
    }

    function setAuxiliaryOverlayVisible(visible: boolean, clear = false): void {
        const loader = document.getElementById("loader");
        const session = document.getElementById("session");
        const rng = document.getElementById("rng");

        if (loader != null) {
            loader.style.opacity = "0";
        }
        if (session != null) {
            session.style.opacity = visible ? "0.6" : "0";
        }
        if (rng != null) {
            rng.style.opacity = visible ? "1" : "0";
        }

        if (!clear) {
            return;
        }

        setTimeout(() => {
            const nextSession = document.getElementById("session");
            const nextRng = document.getElementById("rng");
            const nextLoader = document.getElementById("loader");

            if (nextSession != null) {
                nextSession.innerHTML = "";
            }
            if (nextRng != null) {
                nextRng.innerHTML = "";
            }
            if (nextLoader != null) {
                nextLoader.style.opacity = "0";
                nextLoader.innerHTML = "";
            }
        }, overlayHideFadeMs);
    }

    function hideOverlay(immediate = false): void {
        hideStatsPanel(immediate);
        setOverlayBackgroundVisible(false, immediate);
        setChartVisibility({ visible: false, immediate });
        setAuxiliaryOverlayVisible(false, true);
    }

    function showOverlay(immediate = false): void {
        cancelReplayDisplayClearTimer();
        const stats = document.getElementById("stats");
        if (stats != null) {
            cancelOverlayHideTimer();
            stats.style.display = "block";
            stats.style.right = "1vh";
            stats.style.opacity = "1";
        }
        setOverlayBackgroundVisible(true, immediate);

        setChartVisibility({
            visible: chartVisibleFromConfig && gameStatPayload !== null,
            immediate,
        });

        if (immediate) {
            setAuxiliaryOverlayVisible(true);
            return;
        }

        setTimeout(() => {
            setAuxiliaryOverlayVisible(true);
        }, 1000);
    }

    function showPlayerStats(immediate = false): void {
        hideStatsPanel(immediate);
        setOverlayBackgroundVisible(false, true);
        setChartVisibility({
            visible: false,
            immediate: true,
        });
        setAuxiliaryOverlayVisible(false);
    }

    function colorPreviewEventHandler({
        payload,
    }: {
        payload: OverlayColorPreviewPayload;
    }): void {
        setColors(
            payload.color_player1 ?? null,
            payload.color_player2 ?? null,
            payload.color_amon ?? null,
            payload.color_mastery ?? null,
        );
    }

    function languagePreviewEventHandler({
        payload,
    }: {
        payload: OverlayLanguagePreviewPayload;
    }): void {
        applyOverlayLanguage(payload.language);
    }

    function replayPayloadEventHandler({
        payload,
    }: {
        payload: OverlayReplayPayload;
    }): void {
        setPlayerStatPayload(null);
        setGameStatPayload(payload);
        setSessionVictoryCount(Number(payload.Victory ?? 0));
        setSessionDefeatCount(Number(payload.Defeat ?? 0));
    }

    function togglePlayerStatsEventHandler({
        payload,
    }: {
        payload: OverlayPlayerInfoPayload;
    }): void {
        togglePlayerStatsDisplay(payload, true);
    }

    function playerStatsOnGameStartEventHandler({
        payload,
    }: {
        payload: OverlayPlayerInfoPayload;
    }): void {
        setGameStatPayload(null);
        requestDisplayTransition(DisplayMode.PlayerStats, {
            immediate: true,
            playerPayload: payload,
        });
    }

    function initColorsDurationEventHandler({
        payload,
    }: {
        payload: OverlayInitColorsDurationPayload;
    }): void {
        applyOverlayLanguage(payload.language);
        setChartVisibleFromConfig(payload.show_charts);
        setShowSessionStats(payload.show_session);
        setHideNicknamesInOverlay(payload.hide_nicknames_in_overlay);
        setSessionVictoryCount(payload.session_victory);
        setSessionDefeatCount(payload.session_defeat);
        setColors(
            payload.colors[0],
            payload.colors[1],
            payload.colors[2],
            payload.colors[3],
        );
    }

    function showGameStatsEventHandler(): void {
        requestDisplayTransition(DisplayMode.GameStats);
    }

    function hideGameStatsEventHandler(): void {
        requestDisplayTransition(DisplayMode.None);
    }

    function toggleOverlayEventHandler(): void {
        toggleGameStatsDisplay();
    }

    function setShowChartsFromConfigEventHandler({
        payload,
    }: {
        payload: boolean;
    }): void {
        setChartVisibleFromConfig(payload);
        setChartVisibility({
            visible: payload,
            immediate: true,
        });
    }

    async function saveOverlayScreenshot(path: string): Promise<void> {
        const target = document.body;
        if (target == null) {
            throw new Error("Overlay screenshot root was not found");
        }
        const width = Math.max(window.innerWidth, 1);
        const height = Math.max(window.innerHeight, 1);

        const captureScale = Math.min(
            Math.max(window.devicePixelRatio || 1, 2),
            3,
        );
        set_overlay_chart_pixel_ratio(captureScale);
        await waitForAnimationFrame();
        await waitForAnimationFrame();

        try {
            const canvas = await snapdom.toCanvas(target);
            const dataUrl = canvas.toDataURL("image/png");
            const base64 = dataUrl.replace(/^data:image\/png;base64,/, "");
            const binary = window.atob(base64);
            const buffer = new Uint8Array(binary.length);
            for (let index = 0; index < binary.length; index += 1) {
                buffer[index] = binary.charCodeAt(index);
            }
            const pngBytes = Array.from(buffer);
            await invoke("save_overlay_screenshot", {
                path,
                pngBytes,
            });
        } finally {
            reset_overlay_chart_pixel_ratio();
            await waitForAnimationFrame();
        }
    }

    function overlayScreenshotRequestEventHandler({
        payload,
    }: {
        payload: OverlayScreenshotRequestPayload;
    }): void {
        void (async () => {
            try {
                await saveOverlayScreenshot(payload.path);
                await emit<OverlayScreenshotResultPayload>(
                    OVERLAY_SCREENSHOT_RESULT_EVENT,
                    {
                        ok: true,
                        path: payload.path,
                        message: `Overlay screenshot saved to ${payload.path}`,
                    },
                );
            } catch (error) {
                const message = formatOverlayScreenshotError(error);
                await emit<OverlayScreenshotResultPayload>(
                    OVERLAY_SCREENSHOT_RESULT_EVENT,
                    {
                        ok: false,
                        path: payload.path,
                        message,
                    },
                );
            }
        })();
    }

    async function initializeOverlay(): Promise<void> {
        if (overlayRuntimeStarted) {
            return;
        }

        setOverlayRuntimeStarted(true);

        await Promise.all([
            listen<OverlayColorPreviewPayload>(
                OVERLAY_COLOR_PREVIEW_EVENT,
                colorPreviewEventHandler,
            ).then((unlisten) => {
                tauriUnlistens[OVERLAY_COLOR_PREVIEW_EVENT]?.();
                tauriUnlistens[OVERLAY_COLOR_PREVIEW_EVENT] = unlisten;
            }),
            listen<OverlayLanguagePreviewPayload>(
                OVERLAY_LANGUAGE_PREVIEW_EVENT,
                languagePreviewEventHandler,
            ).then((unlisten) => {
                tauriUnlistens[OVERLAY_LANGUAGE_PREVIEW_EVENT]?.();
                tauriUnlistens[OVERLAY_LANGUAGE_PREVIEW_EVENT] = unlisten;
            }),
            listen<OverlayReplayPayload>(
                OVERLAY_REPLAY_PAYLOAD_EVENT,
                replayPayloadEventHandler,
            ).then((unlisten) => {
                tauriUnlistens[OVERLAY_REPLAY_PAYLOAD_EVENT]?.();
                tauriUnlistens[OVERLAY_REPLAY_PAYLOAD_EVENT] = unlisten;
            }),
            listen<OverlayPlayerInfoPayload>(
                OVERLAY_SHOW_HIDE_PLAYER_WINRATE_EVENT,
                togglePlayerStatsEventHandler,
            ).then((unlisten) => {
                tauriUnlistens[OVERLAY_SHOW_HIDE_PLAYER_WINRATE_EVENT]?.();
                tauriUnlistens[OVERLAY_SHOW_HIDE_PLAYER_WINRATE_EVENT] =
                    unlisten;
            }),
            listen<OverlayPlayerInfoPayload>(
                OVERLAY_PLAYER_WINRATE_EVENT,
                playerStatsOnGameStartEventHandler,
            ).then((unlisten) => {
                tauriUnlistens[OVERLAY_PLAYER_WINRATE_EVENT]?.();
                tauriUnlistens[OVERLAY_PLAYER_WINRATE_EVENT] = unlisten;
            }),
            listen<OverlayInitColorsDurationPayload>(
                OVERLAY_INIT_COLORS_DURATION_EVENT,
                initColorsDurationEventHandler,
            ).then((unlisten) => {
                tauriUnlistens[OVERLAY_INIT_COLORS_DURATION_EVENT]?.();
                tauriUnlistens[OVERLAY_INIT_COLORS_DURATION_EVENT] = unlisten;
            }),
            listen(OVERLAY_SHOWSTATS_EVENT, showGameStatsEventHandler).then(
                (unlisten) => {
                    tauriUnlistens[OVERLAY_SHOWSTATS_EVENT]?.();
                    tauriUnlistens[OVERLAY_SHOWSTATS_EVENT] = unlisten;
                },
            ),
            listen(OVERLAY_HIDESTATS_EVENT, hideGameStatsEventHandler).then(
                (unlisten) => {
                    tauriUnlistens[OVERLAY_HIDESTATS_EVENT]?.();
                    tauriUnlistens[OVERLAY_HIDESTATS_EVENT] = unlisten;
                },
            ),
            listen(OVERLAY_SHOWHIDE_EVENT, toggleOverlayEventHandler).then(
                (unlisten) => {
                    tauriUnlistens[OVERLAY_SHOWHIDE_EVENT]?.();
                    tauriUnlistens[OVERLAY_SHOWHIDE_EVENT] = unlisten;
                },
            ),
            listen<boolean>(
                OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT,
                setShowChartsFromConfigEventHandler,
            ).then((unlisten) => {
                tauriUnlistens[OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT]?.();
                tauriUnlistens[OVERLAY_SET_SHOW_CHARTS_FROM_CONFIG_EVENT] =
                    unlisten;
            }),
            listen<OverlayScreenshotRequestPayload>(
                OVERLAY_SCREENSHOT_REQUEST_EVENT,
                overlayScreenshotRequestEventHandler,
            ).then((unlisten) => {
                tauriUnlistens[OVERLAY_SCREENSHOT_REQUEST_EVENT]?.();
                tauriUnlistens[OVERLAY_SCREENSHOT_REQUEST_EVENT] = unlisten;
            }),
        ]);

        const stats = document.getElementById("stats");
        if (stats != null) {
            stats.style.display = "none";
            stats.style.right = "-50.5vh";
            stats.style.opacity = "0";
        }

        setGameStatPayload(null);
        requestDisplayTransition(DisplayMode.None, { immediate: true });
    }

    function destroyOverlayRuntime(): void {
        setOverlayRuntimeStarted(false);

        for (const eventName of Object.keys(
            tauriUnlistens,
        ) as OverlayEventName[]) {
            const unlisten = tauriUnlistens[eventName];
            unlisten?.();
            tauriUnlistens[eventName] = null;
        }

        cancelReplayExpiryTimer();
        cancelReplayDisplayClearTimer();
        destroy_overlay_charts();
        reportOverlayReplayDataState(false);
    }

    async function ensureOverlayRuntimeInitialized(): Promise<void> {
        ensureOverlayAssetsLoaded();
        await initializeOverlay();
    }

    useInsertionEffect(() => {
        ensureOverlayAssetsLoaded();
    }, []);

    useEffect(() => {
        void (async () => {
            ensureOverlayAssetsLoaded();
            const root = document.documentElement;
            const body = document.body;
            root.style.background = "transparent";
            body.style.background = "transparent";
            body.style.margin = "0";
            body.style.overflow = "hidden";

            await ensureOverlayRuntimeInitialized();
        })();

        return () => {
            destroyOverlayRuntime();
        };
    }, []);

    useEffect(() => {
        switch (displayMode.mode) {
            case DisplayMode.None:
                hideOverlay(displayMode.immediate);
                break;
            case DisplayMode.PlayerStats:
                showPlayerStats(displayMode.immediate);
                break;
            case DisplayMode.GameStats:
                showOverlay(displayMode.immediate);
                break;
        }
    }, [displayMode]);

    useEffect(() => {
        if (gameStatPayload == null) {
            cancelReplayExpiryTimer();
            return;
        }

        const showTimer = setTimeout(() => {
            requestDisplayTransition(DisplayMode.GameStats);
        }, 10);

        if (gameStatPayload.newReplay == null) {
            return () => {
                clearTimeout(showTimer);
                cancelReplayExpiryTimer();
            };
        }

        const expireTimer = setTimeout(
            () => {
                requestDisplayTransition(DisplayMode.None);

                const clearTimer = setTimeout(() => {
                    cancelReplayDisplayClearTimer();
                    setGameStatPayload(null);
                }, overlayHideFadeMs);

                setReplayDisplayClearTimer(clearTimer);
            },
            Math.max(gameStatsHideMs - overlayHideFadeMs, 0),
        );

        setReplayExpiryTimer(expireTimer);

        return () => {
            clearTimeout(showTimer);
            clearTimeout(expireTimer);
            setReplayExpiryTimer(null);
        };
    }, [gameStatPayload]);

    useEffect(() => {
        if (
            displayMode.mode !== DisplayMode.PlayerStats ||
            playerStatPayload == null
        ) {
            return;
        }

        const hideTimer = setTimeout(() => {
            requestDisplayTransition(DisplayMode.None, {
                immediate: true,
            });

            setPlayerStatPayload(null);
        }, playerStatsHideMs);

        return () => {
            clearTimeout(hideTimer);
        };
    }, [displayMode.mode, playerStatPayload]);

    return (
        <div id="overlay-screenshot-root">
            <div id="bgdiv" style={{ display: "none", opacity: 0 }}>
                <div id="ibgdiv" style={{ display: "none" }} />
            </div>
            <GameStatMode
                payload={gameStatPayload}
                chartVisibility={chartVisibility}
                replayModeVisible={displayMode.mode === DisplayMode.GameStats}
                showSessionStats={showSessionStats}
                sessionVictoryCount={sessionVictoryCount}
                sessionDefeatCount={sessionDefeatCount}
                p1Color={p1Color}
                p2Color={p2Color}
                amonColor={amonColor}
                masteryColor={masteryColor}
                cancelReplayDisplayClearTimer={cancelReplayDisplayClearTimer}
                overlayCommanderMasteryCatalog={overlayLanguageManager.commanderMasteryData()}
                overlayPrestigeNameCatalog={overlayPrestigeNameCatalog}
                language={language}
                hideNicknamesInOverlay={hideNicknamesInOverlay}
                overlayLanguageManager={overlayLanguageManager}
                reportOverlayReplayDataState={reportOverlayReplayDataState}
            />
            <PlayerStatMode
                payload={playerStatPayload}
                visible={displayMode.mode === DisplayMode.PlayerStats}
                immediate={displayMode.immediate}
                language={language}
                overlayLanguageManager={overlayLanguageManager}
            />
        </div>
    );
}
