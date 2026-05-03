import { expect, test } from "@playwright/test";

test.setTimeout(60000);

async function installLanguageMock(page) {
    await page.addInitScript(() => {
        const cloneJson = (value) => JSON.parse(JSON.stringify(value));
        let settings = {
            account_folder: "fixtures/accounts",
            language: "en",
            main_names: [],
            detailed_analysis_atstart: false,
            rng_choices: {},
        };
        let activeSettings = cloneJson(settings);
        const randomizerCatalog = {
            commander_mastery: {},
            prestige_names: {},
        };
        const monitorCatalog = [{ index: 1, label: "1 - Primary Monitor" }];
        const configPayload = () => ({
            status: "ok",
            settings,
            active_settings: activeSettings,
            randomizer_catalog: randomizerCatalog,
            monitor_catalog: monitorCatalog,
        });
        const statsPayload = () => ({
            status: "ok",
            ready: true,
            games: 1,
            analysis_running: false,
            analysis_running_mode: null,
            message: "",
            query: "",
            main_handles: ["3-S2-1-900001"],
            commander_mastery: {},
            prestige_names: {},
            analysis: {
                MapData: {
                    "Miner Evacuation": {
                        average_victory_time: 1041,
                        frequency: 1,
                        Victory: 1,
                        Defeat: 0,
                        Winrate: 1,
                        bonus: 0,
                        Fastest: {
                            length: 1041,
                            file: "fixtures/replays/miner-evacuation.SC2Replay",
                            date: 1538345544,
                            difficulty: "Normal",
                            enemy_race: "테란",
                            players: [
                                {
                                    name: "Main Tester",
                                    handle: "3-S2-1-900001",
                                    commander: "Abathur",
                                    apm: 123,
                                    mastery_level: 0,
                                    masteries: [0, 0, 0, 0, 0, 0],
                                    prestige: 0,
                                    prestige_name: "Evolution Master",
                                },
                                {
                                    name: "Partner Tester",
                                    handle: "3-S2-1-900002",
                                    commander: "Swann",
                                    apm: 83,
                                    mastery_level: 0,
                                    masteries: [0, 0, 0, 0, 0, 0],
                                    prestige: 0,
                                    prestige_name: "Chief Engineer",
                                },
                            ],
                        },
                    },
                },
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
            invoke: async (command, request) => {
                if (command === "plugin:app|version") {
                    return "0.1.0";
                }
                if (command === "plugin:event|listen") {
                    return 1;
                }
                if (command === "plugin:event|unlisten") {
                    return null;
                }
                if (command === "plugin:event|emit") {
                    return null;
                }
                if (command === "is_dev") {
                    return true;
                }
                if (command === "config_get") {
                    return configPayload();
                }
                if (command === "config_update") {
                    const nextSettings = request?.settings || activeSettings;
                    activeSettings = cloneJson(nextSettings);
                    if (request?.persist !== false) {
                        settings = cloneJson(nextSettings);
                        activeSettings = cloneJson(nextSettings);
                    }
                    return configPayload();
                }
                if (command === "config_stats_get") {
                    return statsPayload();
                }
                if (command === "config_action") {
                    return {
                        status: "ok",
                        result: { ok: true },
                        message: "ok",
                    };
                }
                if (command === "config_stats_action") {
                    return { status: "ok", message: "ok" };
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
                    return {
                        status: "ok",
                        weeklies: [],
                    };
                }
                if (command !== "config_request") {
                    throw new Error(`Unexpected command: ${command}`);
                }

                const { path, method } = request;
                if (method === "GET" && path === "/config") {
                    return configPayload();
                }

                if (method === "POST" && path === "/config") {
                    const nextSettings =
                        request.body?.settings || activeSettings;
                    activeSettings = cloneJson(nextSettings);
                    if (request.body?.persist !== false) {
                        settings = cloneJson(nextSettings);
                        activeSettings = cloneJson(nextSettings);
                    }
                    return configPayload();
                }

                if (method === "GET" && path.startsWith("/config/stats?")) {
                    return {
                        status: "ok",
                        stats: statsPayload(),
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
                listen: async () => () => {},
            },
            transformCallback: (callback) => {
                const id = Math.floor(Math.random() * 1000000);
                window[`_${id}`] = callback;
                return id;
            },
        };
    });
}

test("language selector switches replay labels between english and korean", async ({
    page,
}) => {
    await installLanguageMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.getByRole("tab", { name: "Statistics" }).click();
    await expect(
        page.getByText("17:21 | Terran", { exact: true }),
    ).toBeVisible();

    await page.getByRole("tab", { name: "Settings" }).click();
    const languageSelect = page.locator("select").first();
    await expect(languageSelect.locator('option[value="en"]')).toHaveText(
        "English",
    );
    await expect(languageSelect.locator('option[value="ko"]')).toHaveText(
        "한국어",
    );

    await languageSelect.selectOption("ko");

    await expect(page.getByRole("button", { name: "저장" })).toBeVisible();
    await expect(languageSelect.locator('option[value="en"]')).toHaveText(
        "English",
    );
    await expect(languageSelect.locator('option[value="ko"]')).toHaveText(
        "한국어",
    );

    await page.getByRole("tab", { name: "통계" }).click();
    await expect(page.getByText("17:21 | 테란", { exact: true })).toBeVisible();
});
