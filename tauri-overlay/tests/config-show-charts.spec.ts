import { expect, test, type Page } from "@playwright/test";

async function installConfigTauriMock(page: Page) {
    await page.addInitScript(() => {
        type ConfigSettings = TestJsonObject;
        type ConfigRequest = TestTauriRequest & {
            path: string;
            method: string;
            body?: {
                settings?: ConfigSettings;
                persist?: boolean;
            };
        };
        const runtimeWindow = window;
        const cloneJson = <T>(value: T): T => JSON.parse(JSON.stringify(value));
        let settings: ConfigSettings = {
            account_folder: "fixtures/accounts",
            main_names: [],
            detailed_analysis_atstart: false,
            rng_choices: {},
        };
        let activeSettings = cloneJson(settings);

        runtimeWindow.__SCO_CONFIG_APPLY_REQUESTS__ = [];
        runtimeWindow.__SCO_CONFIG_SAVE_REQUESTS__ = [];
        runtimeWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
            unregisterListener: () => {},
        };
        runtimeWindow.__TAURI_INTERNALS__ = {
            transformCallback: () => 1,
            unregisterCallback: () => {},
            invoke: async (command: string, request: ConfigRequest) => {
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

                if (
                    command === "config_get" ||
                    (command === "config_request" &&
                        request.method === "GET" &&
                        request.path === "/config")
                ) {
                    return {
                        status: "ok",
                        settings,
                        active_settings: activeSettings,
                        randomizer_catalog: {},
                        monitor_catalog: [
                            { index: 1, label: "1 - Primary Monitor" },
                        ],
                    };
                }

                if (
                    command === "config_update" ||
                    (command === "config_request" &&
                        request.method === "POST" &&
                        request.path === "/config")
                ) {
                    const nextSettings =
                        request.settings ||
                        request.body?.settings || activeSettings;
                    activeSettings = cloneJson(nextSettings);
                    if (
                        request.persist === false ||
                        request.body?.persist === false
                    ) {
                        runtimeWindow.__SCO_CONFIG_APPLY_REQUESTS__.push(
                            activeSettings,
                        );
                    } else {
                        settings = cloneJson(nextSettings);
                        activeSettings = cloneJson(nextSettings);
                        runtimeWindow.__SCO_CONFIG_SAVE_REQUESTS__.push(
                            settings,
                        );
                    }
                    return {
                        status: "ok",
                        settings,
                        active_settings: activeSettings,
                        randomizer_catalog: {},
                        monitor_catalog: [
                            { index: 1, label: "1 - Primary Monitor" },
                        ],
                    };
                }

                if (
                    command === "config_action" ||
                    command === "config_stats_action" ||
                    request.method === "POST"
                ) {
                    return {
                        status: "ok",
                        result: { ok: true },
                        message: "ok",
                    };
                }

                if (command === "config_stats_get") {
                    return {
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
                    };
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
                    };
                }

                if (command === "config_weeklies_get") {
                    return {
                        status: "ok",
                        weeklies: [],
                    };
                }

                return {
                    status: "ok",
                    replays: [],
                    players: [],
                    weeklies: [],
                    stats: {
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
                    },
                };
            },
        };
    });
}

test.describe.configure({ timeout: 60_000 });

test("show charts defaults to enabled and saves the replay overlay toggle", async ({
    page,
}) => {
    await installConfigTauriMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    const showCharts = page.getByRole("checkbox", { name: "Show charts" });
    await expect(showCharts).toBeChecked();

    await showCharts.uncheck();

    await expect
        .poll(() =>
            page.evaluate(() => {
                const runtimeWindow = window as Window & {
                    __SCO_CONFIG_APPLY_REQUESTS__?: TestJsonObject[];
                };
                const requests =
                    runtimeWindow.__SCO_CONFIG_APPLY_REQUESTS__ || [];
                return requests[requests.length - 1] || null;
            }),
        )
        .toMatchObject({
            show_charts: false,
        });

    await page.getByRole("button", { name: /^Save$/ }).click();

    await expect
        .poll(() =>
            page.evaluate(() => {
                const runtimeWindow = window as Window & {
                    __SCO_CONFIG_SAVE_REQUESTS__?: TestJsonObject[];
                };
                const requests =
                    runtimeWindow.__SCO_CONFIG_SAVE_REQUESTS__ || [];
                return requests[requests.length - 1] || null;
            }),
        )
        .toMatchObject({
            show_charts: false,
        });
});
