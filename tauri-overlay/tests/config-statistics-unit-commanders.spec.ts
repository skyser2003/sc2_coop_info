import { expect, test, type Page } from "@playwright/test";

type UnitStatsRow = {
    created: number | string;
    made: number;
    lost: number | string;
    lost_percent: number | null;
    kills: number;
    KD: number | null;
    kill_percentage: number;
};

type UnitCommanderRows = {
    count: number;
    [unitName: string]: UnitStatsRow | number;
};

type CommanderStatsRow = {
    Frequency: number;
    Victory: number;
    Defeat: number;
    Winrate: number;
    MedianAPM: number;
    KillFraction: number;
    Mastery: Record<string, number>;
    MasteryDistribution: Record<string, Record<string, number>>;
    MasteryDistributionByPrestige: Record<
        string,
        Record<string, Record<string, number>>
    >;
    Prestige: Record<string, number>;
    MasteryByPrestige: Record<string, Record<string, number>>;
    detailedCount: number;
};

type PrestigeLabels = {
    en: readonly string[];
    ko: readonly string[];
};

type StatisticsUnitCommanderMockPayload = {
    status: "ok";
    ready: boolean;
    games: number;
    detailed_parsed_count: number;
    total_valid_files: number;
    analysis_running: boolean;
    analysis_running_mode: string | null;
    detailed_analysis_atstart: boolean;
    simple_analysis_status: string;
    detailed_analysis_status: string;
    main_players: readonly string[];
    main_handles: readonly string[];
    prestige_names: Record<string, PrestigeLabels>;
    message: string;
    query: string;
    scan_progress: {
        stage: string;
        status: string;
        parsing_status: string;
        total: number;
        total_replay_files: number;
        cache_hits: number;
        files_already_cached: number;
        to_parse: number;
        completed: number;
        newly_parsed: number;
        newly_parsed_files: number;
        failed: number;
        parse_failed_files: number;
        parse_skipped: number;
        parse_skipped_files: number;
        elapsed_ms: number;
        total_time_taken_ms: number;
    };
    analysis: {
        MapData: Record<string, never>;
        CommanderData: Record<string, CommanderStatsRow>;
        AllyCommanderData: Record<string, CommanderStatsRow>;
        DifficultyData: Record<string, never>;
        RegionData: Record<string, never>;
        PlayerData: Record<string, never>;
        AmonData: Record<string, never>;
        MapDataReady: boolean;
        UnitData: {
            main: Record<string, UnitCommanderRows | null>;
            ally: Record<string, UnitCommanderRows | null>;
            amon: Record<string, never>;
        };
    };
};

type MockSettings = {
    account_folder: string;
    language: string;
    main_names: readonly string[];
    detailed_analysis_atstart: boolean;
    rng_choices: Record<string, boolean>;
};

type MockRequestBody = {
    settings?: MockSettings;
    persist?: boolean;
};

type MockInvokeRequest = {
    settings?: MockSettings;
    persist?: boolean;
    method?: string;
    path?: string;
    body?: MockRequestBody;
};

declare global {
    interface Window {
        _1?: () => void;
    }
}

const EMPTY_COMMANDER_STATS: CommanderStatsRow = {
    Frequency: 0,
    Victory: 0,
    Defeat: 0,
    Winrate: 0,
    MedianAPM: 0,
    KillFraction: 0,
    Mastery: {},
    MasteryDistribution: {},
    MasteryDistributionByPrestige: {},
    Prestige: {},
    MasteryByPrestige: {},
    detailedCount: 0,
};

function unitCommanderStatsPayload(): StatisticsUnitCommanderMockPayload {
    return {
        status: "ok",
        ready: true,
        games: 4,
        detailed_parsed_count: 2,
        total_valid_files: 4,
        analysis_running: false,
        analysis_running_mode: null,
        detailed_analysis_atstart: false,
        simple_analysis_status: "",
        detailed_analysis_status: "",
        main_players: [],
        main_handles: [],
        prestige_names: {
            Raynor: { en: ["P0"], ko: ["P0"] },
            Kerrigan: { en: ["P0"], ko: ["P0"] },
            Karax: { en: ["P0"], ko: ["P0"] },
        },
        message: "",
        query: "",
        scan_progress: {
            stage: "",
            status: "",
            parsing_status: "",
            total: 0,
            total_replay_files: 0,
            cache_hits: 0,
            files_already_cached: 0,
            to_parse: 0,
            completed: 0,
            newly_parsed: 0,
            newly_parsed_files: 0,
            failed: 0,
            parse_failed_files: 0,
            parse_skipped: 0,
            parse_skipped_files: 0,
            elapsed_ms: 0,
            total_time_taken_ms: 0,
        },
        analysis: {
            MapData: {},
            CommanderData: {
                Raynor: EMPTY_COMMANDER_STATS,
                Kerrigan: EMPTY_COMMANDER_STATS,
            },
            AllyCommanderData: {
                Raynor: EMPTY_COMMANDER_STATS,
                Karax: EMPTY_COMMANDER_STATS,
            },
            DifficultyData: {},
            RegionData: {},
            PlayerData: {},
            AmonData: {},
            MapDataReady: true,
            UnitData: {
                main: {
                    Raynor: {
                        count: 2,
                        Marine: {
                            created: 12,
                            made: 1,
                            lost: 3,
                            lost_percent: 0.25,
                            kills: 30,
                            KD: 10,
                            kill_percentage: 0.7,
                        },
                        sum: {
                            created: 12,
                            made: 1,
                            lost: 3,
                            lost_percent: 0.25,
                            kills: 30,
                            KD: 10,
                            kill_percentage: 1,
                        },
                    },
                    Kerrigan: null,
                },
                ally: {
                    Karax: {
                        count: 1,
                        Zealot: {
                            created: 3,
                            made: 1,
                            lost: 1,
                            lost_percent: 0.3333,
                            kills: 8,
                            KD: 8,
                            kill_percentage: 0.4,
                        },
                        sum: {
                            created: 3,
                            made: 1,
                            lost: 1,
                            lost_percent: 0.3333,
                            kills: 8,
                            KD: 8,
                            kill_percentage: 1,
                        },
                    },
                    Raynor: null,
                },
                amon: {},
            },
        },
    };
}

async function installUnitCommanderStatsMock(
    page: Page,
    statsPayload: StatisticsUnitCommanderMockPayload,
): Promise<void> {
    await page.addInitScript((payload: StatisticsUnitCommanderMockPayload) => {
        const cloneJson = <T extends TestJsonValue>(value: T): T =>
            JSON.parse(JSON.stringify(value)) as T;
        let settings: MockSettings = {
            account_folder: "fixtures/accounts",
            language: "en",
            main_names: [],
            detailed_analysis_atstart: false,
            rng_choices: {},
        };
        let activeSettings = cloneJson(settings);
        const configPayload = () => ({
            status: "ok",
            settings,
            active_settings: activeSettings,
            randomizer_catalog: {
                commander_mastery: {},
                prestige_names: payload.prestige_names,
                mutators: [],
                brutal_plus: [],
            },
            monitor_catalog: [],
        });

        window.__SCO_ACTION_REQUESTS__ = [];
        window.__SCO_CONFIG_GET_REQUESTS__ = [];
        window.__SCO_CONFIG_APPLY_REQUESTS__ = [];
        window.__SCO_CONFIG_SAVE_REQUESTS__ = [];
        window.__SCO_FOLDER_PICKER_REQUESTS__ = [];
        window.__SCO_STATS_REQUESTS__ = [];
        window.__SCO_TAB_REQUESTS__ = [];
        window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
            unregisterListener: () => {},
        };

        window.__TAURI_INTERNALS__ = {
            invoke: async (
                command: string,
                request?: MockInvokeRequest,
            ): Promise<TestJsonValue> => {
                if (command === "plugin:app|version") {
                    return "0.1.0";
                }
                if (command === "plugin:event|listen") {
                    return 1;
                }
                if (
                    command === "plugin:event|unlisten" ||
                    command === "plugin:event|emit"
                ) {
                    return null;
                }
                if (command === "is_dev") {
                    return true;
                }
                if (command === "config_get") {
                    return configPayload();
                }
                if (command === "config_update") {
                    const nextSettings = request?.settings ?? activeSettings;
                    activeSettings = cloneJson(nextSettings);
                    if (request?.persist !== false) {
                        settings = cloneJson(nextSettings);
                        activeSettings = cloneJson(nextSettings);
                    }
                    return configPayload();
                }
                if (command === "config_stats_get") {
                    window.__SCO_STATS_REQUESTS__.push(
                        cloneJson(request ?? {}),
                    );
                    return payload;
                }
                if (command === "config_replays_get") {
                    return {
                        status: "ok",
                        replays: [],
                        total_replays: 0,
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
                    return { status: "ok", weeklies: [] };
                }
                if (command === "config_stats_action") {
                    return { status: "ok", message: "ok" };
                }
                if (command === "config_action") {
                    return {
                        status: "ok",
                        result: { ok: true },
                        message: "ok",
                    };
                }
                if (command !== "config_request") {
                    throw new Error(`Unexpected command: ${command}`);
                }

                const method = request?.method ?? "";
                const path = request?.path ?? "";
                if (method === "GET" && path === "/config") {
                    return configPayload();
                }
                if (method === "POST" && path === "/config") {
                    const nextSettings =
                        request?.body?.settings ?? activeSettings;
                    activeSettings = cloneJson(nextSettings);
                    if (request?.body?.persist !== false) {
                        settings = cloneJson(nextSettings);
                        activeSettings = cloneJson(nextSettings);
                    }
                    return configPayload();
                }
                if (method === "GET" && path.startsWith("/config/stats?")) {
                    return {
                        status: "ok",
                        stats: payload,
                    };
                }
                if (method === "POST" && path === "/config/action") {
                    return {
                        status: "ok",
                        result: { ok: true },
                        message: "ok",
                    };
                }

                throw new Error(`Unexpected request: ${method} ${path}`);
            },
            event: {
                listen: async () => (): void => {},
            },
            transformCallback: (callback: () => void): number => {
                window._1 = callback;
                return 1;
            },
        };
    }, statsPayload);
}

test("unit statistics lists commanders and disables missing unit records", async ({
    page,
}) => {
    await installUnitCommanderStatsMock(page, unitCommanderStatsPayload());
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.getByRole("tab", { name: "Statistics" }).click();
    await page.getByRole("button", { name: "Unit stats" }).click();

    await expect(
        page.getByTestId("unit-commander-main-Raynor"),
    ).toHaveAttribute("aria-disabled", "false");
    await expect(
        page.getByTestId("unit-commander-main-Kerrigan"),
    ).toHaveAttribute("aria-disabled", "true");
    await expect(page.getByTestId("unit-commander-ally-Karax")).toHaveAttribute(
        "aria-disabled",
        "false",
    );
    await expect(
        page.getByTestId("unit-commander-ally-Raynor"),
    ).toHaveAttribute("aria-disabled", "true");

    await expect(
        page.getByRole("heading", {
            name: "Unit stats (Main) - Raynor",
        }),
    ).toBeVisible();
    await page.getByTestId("unit-commander-main-Kerrigan").click({
        force: true,
    });
    await expect(
        page.getByRole("heading", {
            name: "Unit stats (Main) - Raynor",
        }),
    ).toBeVisible();

    await page.getByTestId("unit-commander-ally-Karax").click();
    await expect(
        page.getByRole("heading", {
            name: "Unit stats (Ally) - Karax",
        }),
    ).toBeVisible();
    await expect(page.getByText("Zealot")).toBeVisible();
});
