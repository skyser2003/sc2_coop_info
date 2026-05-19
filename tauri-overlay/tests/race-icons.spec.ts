import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import type {
    AppSettings,
    GamesRowPayload,
    OverlayInitColorsDurationPayload,
    OverlayReplayPayload,
} from "../src/bindings/overlay";

test.describe.configure({ timeout: 60_000 });

type MockOverlayPayload =
    | OverlayInitColorsDurationPayload
    | OverlayReplayPayload
    | Record<string, never>;

type MockOverlayEvent = {
    event: string;
    id: number;
    payload: MockOverlayPayload;
};

type MockInvokeRequest = {
    action?: string;
    event?: string;
    handler?: number;
    limit?: number;
    method?: string;
    path?: string;
    persist?: boolean;
    query?: string;
    settings?: AppSettings;
};

type MockConfig = {
    replays?: readonly GamesRowPayload[];
};

const RASTER_RACE_ICON_ASSETS = ["terran", "zerg", "protoss"] as const;
const FALLBACK_RACE_ICON_ASSET = "unknown" as const;

type EmitMockOverlayEvent = (
    eventName: string,
    payload: MockOverlayPayload,
) => void;

function buildSettings(): AppSettings {
    return {
        start_with_windows: false,
        minimize_to_tray: false,
        start_minimized: false,
        auto_update: false,
        duration: 60,
        show_player_winrates: true,
        show_replay_info_after_game: true,
        show_session: false,
        show_charts: false,
        hide_nicknames_in_overlay: false,
        account_folder: "fixtures/accounts",
        screenshot_folder: "fixtures/screenshots",
        color_player1: "#0080F8",
        color_player2: "#00D532",
        color_amon: "red",
        color_mastery: "#FFDC87",
        "hotkey_show/hide": null,
        hotkey_show: null,
        hotkey_hide: null,
        hotkey_newer: null,
        hotkey_older: null,
        hotkey_winrates: null,
        enable_logging: false,
        dark_theme: true,
        language: "en",
        monitor: 0,
        performance_show: false,
        performance_hotkey: null,
        performance_processes: [],
        rng_choices: {},
        player_notes: {},
        main_names: [],
        detailed_analysis_atstart: false,
        analysis_worker_threads: 1,
        latest_today_win_bonus_time: null,
    };
}

function buildGamesRow(enemy: string, file: string): GamesRowPayload {
    return {
        file,
        date: 1_700_000_000,
        map: "Chain of Ascension",
        result: "Victory",
        difficulty: "Brutal",
        p1: "Player One",
        p2: "Player Two",
        slot1_commander: "Raynor",
        slot2_commander: "Kerrigan",
        enemy,
        main_commander: "Raynor",
        ally_commander: "Kerrigan",
        length: 1200,
        main_apm: 100,
        ally_apm: 90,
        main_kills: 30,
        ally_kills: 20,
        extension: false,
        brutal_plus: 0,
        weekly: false,
        mutators: [],
        is_mutation: false,
    };
}

function buildReplayPayload(comp: string, enemy: string): OverlayReplayPayload {
    return {
        file: "race-icon.SC2Replay",
        map_name: "Chain of Ascension",
        main: "Player One",
        ally: "Player Two",
        mainCommander: "Raynor",
        allyCommander: "Kerrigan",
        mainAPM: 100,
        allyAPM: 90,
        mainkills: 30,
        allykills: 20,
        result: "Victory",
        difficulty: "Brutal",
        length: 100,
        "B+": 0,
        weekly: false,
        extension: false,
        mainCommanderLevel: 15,
        allyCommanderLevel: 15,
        mainMasteryLevel: 90,
        allyMasteryLevel: 90,
        mainMasteries: [0, 0, 0, 0, 0, 0],
        allyMasteries: [0, 0, 0, 0, 0, 0],
        mainUnits: {},
        allyUnits: {},
        amon_units: {},
        mainIcons: {},
        allyIcons: {},
        mutators: [],
        bonus: [],
        mainPrestige: "Renegade Commander",
        allyPrestige: "Queen of Blades",
        Victory: 1,
        Defeat: 0,
        fastest: false,
        comp,
        enemy,
    };
}

async function installConfigMock(
    page: Page,
    { replays = [] }: MockConfig,
): Promise<void> {
    await page.addInitScript(
        ({
            initialReplays,
            settingsPayload,
        }: {
            initialReplays: readonly GamesRowPayload[];
            settingsPayload: AppSettings;
        }) => {
            let activeSettings = JSON.parse(
                JSON.stringify(settingsPayload),
            ) as AppSettings;
            const configPayload = () => ({
                status: "ok",
                settings: settingsPayload,
                active_settings: activeSettings,
                randomizer_catalog: {
                    prestige_names: {},
                    mutators: [],
                    brutal_plus: [],
                },
                monitor_catalog: [],
            });
            const statisticsPayload = () => ({
                status: "ok",
                ready: true,
                games: 0,
                analysis_running: false,
                analysis_running_mode: null,
                message: "",
                query: "",
                analysis: {
                    MapData: {},
                    CommanderData: {},
                    AllyCommanderData: {},
                    DifficultyData: {},
                    RegionData: {},
                    PlayerData: {},
                    AmonData: {},
                    MapDataReady: true,
                    UnitData: {
                        main: {},
                        ally: {},
                        amon: {},
                    },
                },
            });

            window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener: () => {},
            };

            window.__TAURI_INTERNALS__ = {
                invoke: async (
                    command: string,
                    request: MockInvokeRequest = {},
                ) => {
                    if (command === "plugin:app|version") {
                        return "0.1.0";
                    }
                    if (command === "plugin:event|listen") {
                        return 1;
                    }
                    if (command === "plugin:event|unlisten") {
                        return null;
                    }
                    if (command === "is_dev") {
                        return true;
                    }
                    if (command === "config_get") {
                        return configPayload();
                    }
                    if (command === "config_update") {
                        if (request.settings != null) {
                            activeSettings = JSON.parse(
                                JSON.stringify(request.settings),
                            ) as AppSettings;
                        }
                        return configPayload();
                    }
                    if (command === "config_replays_get") {
                        return {
                            status: "ok",
                            replays: initialReplays,
                            total_replays: initialReplays.length,
                            selected_replay_file: "",
                        };
                    }
                    if (command === "config_players_get") {
                        return {
                            status: "ok",
                            players: [],
                            total_players: 0,
                            loading: false,
                        };
                    }
                    if (command === "config_weeklies_get") {
                        return {
                            status: "ok",
                            weeklies: [],
                        };
                    }
                    if (command === "config_stats_get") {
                        return statisticsPayload();
                    }
                    if (
                        command === "config_action" ||
                        command === "config_stats_action"
                    ) {
                        return {
                            status: "ok",
                            result: { ok: true, path: null },
                            message: "ok",
                            randomizer: null,
                            stats: statisticsPayload(),
                        };
                    }

                    throw new Error(`Unexpected command: ${command}`);
                },
                event: {
                    listen: async () => () => {},
                },
                transformCallback: (callback: () => void) => {
                    const id = Math.floor(Math.random() * 1_000_000);
                    const callbackWindow = window as typeof window &
                        Record<string, () => void>;
                    callbackWindow[`_${id}`] = callback;
                    return id;
                },
            };
        },
        {
            initialReplays: replays,
            settingsPayload: buildSettings(),
        },
    );
}

async function installOverlayMock(page: Page): Promise<void> {
    await page.addInitScript(
        ({ settingsPayload }: { settingsPayload: AppSettings }) => {
            const listeners = new Map<string, number[]>();
            const callbacks = new Map<
                number,
                (payload: MockOverlayEvent) => void
            >();
            let nextCallbackId = 1;
            let nextEventListenerId = 1;

            window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener: () => {},
            };

            window.__TAURI_INTERNALS__ = {
                transformCallback: (
                    callback: (payload: MockOverlayEvent) => void,
                ) => {
                    const id = nextCallbackId;
                    nextCallbackId += 1;
                    callbacks.set(id, callback);
                    return id;
                },
                unregisterCallback: (id: number) => {
                    callbacks.delete(id);
                },
                invoke: async (
                    command: string,
                    request: MockInvokeRequest = {},
                ) => {
                    if (command === "plugin:event|listen") {
                        const eventName = request.event ?? "";
                        const handler = request.handler ?? 0;
                        const current = listeners.get(eventName) || [];
                        current.push(handler);
                        listeners.set(eventName, current);
                        return nextEventListenerId++;
                    }
                    if (command === "plugin:event|unlisten") {
                        return null;
                    }
                    if (command === "config_get") {
                        return {
                            status: "ok",
                            settings: settingsPayload,
                            active_settings: settingsPayload,
                            randomizer_catalog: {
                                prestige_names: {},
                                mutators: [],
                                brutal_plus: [],
                            },
                            monitor_catalog: [],
                        };
                    }
                    if (command === "config_action") {
                        return {
                            status: "ok",
                            result: { ok: true, path: null },
                            message: "ok",
                            randomizer: null,
                        };
                    }

                    throw new Error(`Unexpected command: ${command}`);
                },
            };

            (
                window as typeof window & {
                    __emitMockOverlayEvent?: EmitMockOverlayEvent;
                }
            ).__emitMockOverlayEvent = (
                eventName: string,
                payload: MockOverlayPayload,
            ) => {
                for (const callbackId of listeners.get(eventName) || []) {
                    callbacks.get(callbackId)?.({
                        event: eventName,
                        id: callbackId,
                        payload,
                    });
                }
            };
        },
        { settingsPayload: buildSettings() },
    );
}

async function postReplayPayload(
    page: Page,
    payload: OverlayReplayPayload,
): Promise<void> {
    await page.evaluate((nextPayload: OverlayReplayPayload) => {
        const runtime = window as typeof window & {
            __emitMockOverlayEvent?: EmitMockOverlayEvent;
        };

        runtime.__emitMockOverlayEvent?.("sco://overlay-init-colors-duration", {
            colors: [null, null, null, null],
            duration: 60,
            show_charts: false,
            show_session: false,
            hide_nicknames_in_overlay: false,
            session_victory: 0,
            session_defeat: 0,
            language: "en",
        });
        runtime.__emitMockOverlayEvent?.(
            "sco://overlay-replay-payload",
            nextPayload,
        );
    }, payload);
}

test("games tab shows a race icon next to enemy race text", async ({
    page,
}) => {
    await installConfigMock(page, {
        replays: [
            buildGamesRow("Terran", "terran.SC2Replay"),
            buildGamesRow("Zerg", "zerg.SC2Replay"),
            buildGamesRow("Protoss", "protoss.SC2Replay"),
            buildGamesRow("Random", "random.SC2Replay"),
            buildGamesRow("Hybrid", "unknown.SC2Replay"),
        ],
    });

    await page.goto("/#/config/games", { waitUntil: "domcontentloaded" });

    const gamesSection = page
        .getByRole("heading", { name: "Games" })
        .locator("xpath=ancestor::section[1]");
    const rows = gamesSection.locator("tbody tr");
    const terranEnemyCell = rows
        .filter({ hasText: "Terran" })
        .locator("td")
        .nth(4);
    const zergEnemyCell = rows.filter({ hasText: "Zerg" }).locator("td").nth(4);
    const protossEnemyCell = rows
        .filter({ hasText: "Protoss" })
        .locator("td")
        .nth(4);
    const randomEnemyCell = rows
        .filter({ hasText: "Random" })
        .locator("td")
        .nth(4);
    const unknownEnemyCell = rows
        .filter({ hasText: "Hybrid" })
        .locator("td")
        .nth(4);

    await expect(rows).toHaveCount(5);
    await expect(terranEnemyCell).toContainText("Terran");
    await expect(terranEnemyCell.locator("svg[data-race='terran']")).toHaveCSS(
        "color",
        "rgb(37, 99, 235)",
    );
    await expect(
        terranEnemyCell.locator("svg[data-race='terran'] path"),
    ).toHaveCount(0);
    await expect(zergEnemyCell).toContainText("Zerg");
    await expect(zergEnemyCell.locator("svg[data-race='zerg']")).toBeVisible();
    await expect(zergEnemyCell.locator("svg[data-race='zerg']")).toHaveCSS(
        "color",
        "rgb(147, 51, 234)",
    );
    await expect(
        zergEnemyCell.locator("svg[data-race='zerg'] image"),
    ).toHaveAttribute("href", "/race-icons/zerg.svg");
    await expect(
        zergEnemyCell.locator("svg[data-race='zerg'] path"),
    ).toHaveCount(0);
    await expect(protossEnemyCell).toContainText("Protoss");
    await expect(
        protossEnemyCell.locator("svg[data-race='protoss']"),
    ).toBeVisible();
    await expect(
        protossEnemyCell.locator("svg[data-race='protoss']"),
    ).toHaveCSS("color", "rgb(234, 179, 8)");
    await expect(
        protossEnemyCell.locator("svg[data-race='protoss'] image"),
    ).toHaveAttribute("href", "/race-icons/protoss.svg");
    await expect(
        protossEnemyCell.locator("svg[data-race='protoss'] path"),
    ).toHaveCount(0);
    await expect(randomEnemyCell).toContainText("Random");
    await expect(
        randomEnemyCell.locator("svg[data-race='random'] image"),
    ).toHaveAttribute("href", "/race-icons/unknown.svg");
    await expect(
        randomEnemyCell.locator("svg[data-race='random'] path"),
    ).toHaveCount(0);
    await expect(unknownEnemyCell).toContainText("Hybrid");
    await expect(
        unknownEnemyCell.locator("svg[data-race='unknown'] image"),
    ).toHaveAttribute("href", "/race-icons/unknown.svg");
    await expect(
        unknownEnemyCell.locator("svg[data-race='unknown'] path"),
    ).toHaveCount(0);
});

test("race icon assets use plain silhouette masks", () => {
    for (const race of RASTER_RACE_ICON_ASSETS) {
        const assetPath = join(
            process.cwd(),
            "public",
            "race-icons",
            `${race}.svg`,
        );
        const asset = readFileSync(assetPath, "utf8");

        expect(asset).toContain("data:image/png;base64");
        expect(asset).not.toContain("data:image/webp");
        expect(asset).not.toContain("<circle");
    }

    const fallbackAssetPath = join(
        process.cwd(),
        "public",
        "race-icons",
        `${FALLBACK_RACE_ICON_ASSET}.svg`,
    );
    const fallbackAsset = readFileSync(fallbackAssetPath, "utf8");

    expect(fallbackAsset).toContain("<path");
    expect(fallbackAsset).toContain('stroke="#fff"');
    expect(fallbackAsset).not.toContain("data:image");
    expect(fallbackAsset).not.toContain("<circle");
});

test("replay overlay uses enemy race data for the composition icon", async ({
    page,
}) => {
    await installOverlayMock(page);
    await page.goto("/#/overlay", { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#stats", { state: "attached" });

    await postReplayPayload(
        page,
        buildReplayPayload("Masters and Machines", "Terran"),
    );

    const composition = page.locator("#comp");

    await expect(composition).toContainText("Masters and Machines");
    await expect(
        composition.locator("svg.enemy-race-icon[data-race='terran']"),
    ).toBeVisible();
    await expect(
        composition.locator("svg.enemy-race-icon[data-race='terran']"),
    ).toHaveCSS("color", "rgb(96, 165, 250)");
    await expect(
        composition.locator("svg.enemy-race-icon[data-race='terran'] image"),
    ).toHaveAttribute("href", "/race-icons/terran.svg");
});
