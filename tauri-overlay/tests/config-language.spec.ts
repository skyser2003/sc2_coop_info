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

        window.__TAURI_INTERNALS__ = {
            invoke: async (command, request) => {
                if (command !== "config_request") {
                    throw new Error(`Unexpected command: ${command}`);
                }

                const { path, method } = request;
                if (method === "GET" && path === "/config") {
                    return {
                        status: "ok",
                        settings,
                        active_settings: activeSettings,
                        randomizer_catalog: {
                            commander_mastery: {},
                            prestige_names: {},
                        },
                        monitor_catalog: [
                            { index: 1, label: "1 - Primary Monitor" },
                        ],
                    };
                }

                if (method === "POST" && path === "/config") {
                    const nextSettings =
                        request.body?.settings || activeSettings;
                    activeSettings = cloneJson(nextSettings);
                    if (request.body?.persist !== false) {
                        settings = cloneJson(nextSettings);
                        activeSettings = cloneJson(nextSettings);
                    }
                    return {
                        status: "ok",
                        settings,
                        active_settings: activeSettings,
                        randomizer_catalog: {
                            commander_mastery: {},
                            prestige_names: {},
                        },
                        monitor_catalog: [
                            { index: 1, label: "1 - Primary Monitor" },
                        ],
                    };
                }

                if (method === "GET" && path.startsWith("/config/stats?")) {
                    return {
                        status: "ok",
                        stats: {
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
                                                    masteries: [
                                                        0, 0, 0, 0, 0, 0,
                                                    ],
                                                    prestige: 0,
                                                    prestige_name:
                                                        "Evolution Master",
                                                },
                                                {
                                                    name: "Partner Tester",
                                                    handle: "3-S2-1-900002",
                                                    commander: "Swann",
                                                    apm: 83,
                                                    mastery_level: 0,
                                                    masteries: [
                                                        0, 0, 0, 0, 0, 0,
                                                    ],
                                                    prestige: 0,
                                                    prestige_name:
                                                        "Chief Engineer",
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
                        },
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
        };
    });
}

test("language selector switches replay labels between english and korean", async ({
    page,
}) => {
    await installLanguageMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.locator('button[data-tab="statistics"]').click();
    await expect(page.locator(".stats-map-sub")).toHaveText("17:21 | Terran");

    await page.locator('button[data-tab="settings"]').click();
    const languageSelect = page
        .locator(".main-settings-inline-numbers select")
        .first();
    await expect(languageSelect.locator('option[value="en"]')).toHaveText(
        "English",
    );
    await expect(languageSelect.locator('option[value="ko"]')).toHaveText(
        "한국어",
    );

    await languageSelect.selectOption("ko");

    await expect(page.getByRole("button", { name: "적용" })).toBeVisible();
    await expect(languageSelect.locator('option[value="en"]')).toHaveText(
        "English",
    );
    await expect(languageSelect.locator('option[value="ko"]')).toHaveText(
        "한국어",
    );

    await page.locator('button[data-tab="statistics"]').click();
    await expect(page.locator(".stats-map-sub")).toHaveText("17:21 | 테란");
});
