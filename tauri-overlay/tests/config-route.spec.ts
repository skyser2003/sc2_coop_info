import { test, expect } from "@playwright/test";

async function installTauriMock(
    page,
    statsPayload = null,
    statsQueryPayloads = [],
    extra = {},
) {
    await page.addInitScript(
        ({ payload, queryPayloads, overrides }) => {
            const cloneJson = (value) => JSON.parse(JSON.stringify(value));
            let settings = {
                account_folder: "fixtures/accounts",
                main_names: [],
                detailed_analysis_atstart: false,
                rng_choices: {},
                ...((overrides && overrides.settings) || {}),
            };
            let activeSettings = cloneJson(settings);
            const randomizerCatalog = (overrides &&
                overrides.randomizerCatalog) || {
                commander_mastery: {
                    Abathur: [
                        "Toxic Nest Damage",
                        "Mend Healing Duration",
                        "Symbiote Ability Improvement",
                        "Double Biomass Chance",
                        "Toxic Nest Maximum Charges and Cooldown",
                        "Structure Morph and Evolution Rate",
                    ],
                    Fenix: [
                        "Fenix Suit Attack Speed",
                        "Fenix Suit Offline Energy Regeneration",
                        "Champion A.I. Attack Speed",
                        "Champion A.I. Life and Shields",
                        "Chrono Boost Efficiency",
                        "Extra Starting Supply",
                    ],
                },
                prestige_names: {
                    Abathur: {
                        en: [
                            "Evolution Master",
                            "Essence Hoarder",
                            "Tunneling Horror",
                            "The Limitless",
                        ],
                        ko: ["진화 군주", "정수 축적가", "땅굴 공포", "무제한"],
                    },
                    Fenix: {
                        en: [
                            "Purifier Executor",
                            "Akhundelar",
                            "Network Administrator",
                            "Unconquered Spirit",
                        ],
                        ko: [
                            "정화자 집행관",
                            "아쿤델라르",
                            "네트워크 관리자",
                            "굴하지 않는 정신",
                        ],
                    },
                },
            };
            const actionResponses =
                (overrides && overrides.actionResponses) || {};
            const folderPickerResponses =
                (overrides && overrides.folderPickerResponses) || {};
            const tabResponses = (overrides && overrides.tabResponses) || {};
            const monitorCatalog = (overrides && overrides.monitorCatalog) || [
                { index: 1, label: "1 - Primary Monitor" },
                { index: 2, label: "2 - Secondary Monitor" },
            ];
            window.__SCO_ACTION_REQUESTS__ = [];
            window.__SCO_CONFIG_APPLY_REQUESTS__ = [];
            window.__SCO_CONFIG_SAVE_REQUESTS__ = [];
            window.__SCO_FOLDER_PICKER_REQUESTS__ = [];
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
                    if (command === "is_dev") {
                        return true;
                    }
                    if (command === "pick_folder") {
                        window.__SCO_FOLDER_PICKER_REQUESTS__.push(
                            request || null,
                        );
                        const directory = request?.directory || "";
                        if (
                            Object.prototype.hasOwnProperty.call(
                                folderPickerResponses,
                                directory,
                            )
                        ) {
                            return folderPickerResponses[directory];
                        }
                        if (
                            Object.prototype.hasOwnProperty.call(
                                folderPickerResponses,
                                "__default",
                            )
                        ) {
                            return folderPickerResponses.__default;
                        }
                        return null;
                    }
                    if (command === "config_get") {
                        return {
                            status: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: randomizerCatalog,
                            monitor_catalog: monitorCatalog,
                        };
                    }
                    if (command === "config_update") {
                        const nextSettings =
                            request?.settings || activeSettings;
                        activeSettings = cloneJson(nextSettings);
                        if (request?.persist === false) {
                            window.__SCO_CONFIG_APPLY_REQUESTS__.push(
                                activeSettings,
                            );
                        } else {
                            settings = cloneJson(nextSettings);
                            activeSettings = cloneJson(nextSettings);
                            window.__SCO_CONFIG_SAVE_REQUESTS__.push(settings);
                        }
                        return {
                            status: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: randomizerCatalog,
                            monitor_catalog: monitorCatalog,
                        };
                    }
                    if (command === "config_stats_get") {
                        const query = request?.query || "";
                        const matched = Array.isArray(queryPayloads)
                            ? queryPayloads.find((entry) =>
                                  query.includes(entry.match),
                              )
                            : null;
                        if (matched?.response) {
                            return cloneJson(
                                matched.response.stats || matched.response,
                            );
                        }
                        if (payload) {
                            return cloneJson(payload.stats || payload);
                        }
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
                    if (command === "config_stats_action") {
                        return { status: "ok", message: "ok" };
                    }
                    if (command === "config_replays_get") {
                        return (
                            tabResponses.games || {
                                status: "ok",
                                replays: [],
                                selected_replay_file: "",
                            }
                        );
                    }
                    if (command === "config_players_get") {
                        return (
                            tabResponses.players || {
                                status: "ok",
                                players: [],
                            }
                        );
                    }
                    if (command === "config_weeklies_get") {
                        return (
                            tabResponses.weeklies || {
                                status: "ok",
                                weeklies: [],
                            }
                        );
                    }
                    if (command === "config_action") {
                        window.__SCO_ACTION_REQUESTS__.push(request || null);
                        const action = request?.action;
                        if (action && actionResponses[action]) {
                            return actionResponses[action];
                        }
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
                        };
                    }
                    if (command !== "config_request") {
                        throw new Error(`Unexpected command: ${command}`);
                    }
                    const { path, method } = request;
                    if (method === "GET" && path === "/config") {
                        return {
                            status: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: randomizerCatalog,
                            monitor_catalog: monitorCatalog,
                        };
                    }
                    if (method === "POST" && path === "/config") {
                        const nextSettings =
                            request.body?.settings || activeSettings;
                        activeSettings = cloneJson(nextSettings);
                        if (request.body?.persist === false) {
                            window.__SCO_CONFIG_APPLY_REQUESTS__.push(
                                activeSettings,
                            );
                        } else {
                            settings = cloneJson(nextSettings);
                            activeSettings = cloneJson(nextSettings);
                            window.__SCO_CONFIG_SAVE_REQUESTS__.push(settings);
                        }
                        return {
                            status: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: randomizerCatalog,
                            monitor_catalog: monitorCatalog,
                        };
                    }
                    if (method === "POST" && path === "/config/stats/action") {
                        return { status: "ok", message: "ok" };
                    }
                    if (method === "POST" && path === "/config/action") {
                        window.__SCO_ACTION_REQUESTS__.push(
                            request.body || null,
                        );
                        const action = request.body?.action;
                        if (action && actionResponses[action]) {
                            return actionResponses[action];
                        }
                        if (action === "randomizer_generate") {
                            return {
                                status: "ok",
                                result: { ok: true },
                                message: "Generated random commander",
                                randomizer: {
                                    commander: "Fenix",
                                    prestige: 0,
                                    prestige_name: "Purifier Executor",
                                    mastery: [
                                        {
                                            points: 0,
                                            label: "Fenix Suit Attack Speed",
                                        },
                                        {
                                            points: 30,
                                            label: "Fenix Suit Offline Energy Regeneration",
                                        },
                                        {
                                            points: 30,
                                            label: "Champion A.I. Attack Speed",
                                        },
                                        {
                                            points: 0,
                                            label: "Champion A.I. Life and Shields",
                                        },
                                        {
                                            points: 30,
                                            label: "Chrono Boost Efficiency",
                                        },
                                        {
                                            points: 0,
                                            label: "Extra Starting Supply",
                                        },
                                    ],
                                    map_race: "Scythe of Amon | Zerg",
                                },
                            };
                        }
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
                        };
                    }
                    if (
                        method === "GET" &&
                        path.startsWith("/config/replays?")
                    ) {
                        return (
                            tabResponses.games || {
                                status: "ok",
                                replays: [],
                                selected_replay_file: "",
                            }
                        );
                    }
                    if (
                        method === "GET" &&
                        path.startsWith("/config/players?")
                    ) {
                        return (
                            tabResponses.players || {
                                status: "ok",
                                players: [],
                            }
                        );
                    }
                    if (method === "GET" && path === "/config/weeklies") {
                        return (
                            tabResponses.weeklies || {
                                status: "ok",
                                weeklies: [],
                            }
                        );
                    }
                    if (method === "GET" && path.startsWith("/config/stats?")) {
                        const matched = Array.isArray(queryPayloads)
                            ? queryPayloads.find((entry) =>
                                  path.includes(entry.match),
                              )
                            : null;
                        if (matched) {
                            return matched.response;
                        }
                        return (
                            payload || {
                                status: "ok",
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
                            }
                        );
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
        },
        {
            payload: statsPayload,
            queryPayloads: statsQueryPayloads,
            overrides: extra,
        },
    );
}

test.describe("Config route", () => {
    test.describe.configure({ timeout: 60000 });

    test("commander mastery statistics render same-category distribution graphs", async ({
        page,
    }) => {
        await installTauriMock(page, {
            status: "ok",
            stats: {
                ready: true,
                games: 10,
                analysis_running: false,
                analysis_running_mode: null,
                message: "",
                query: "",
                analysis: {
                    MapData: {},
                    CommanderData: {
                        Abathur: {
                            Frequency: 1,
                            Victory: 8,
                            Defeat: 2,
                            Winrate: 0.8,
                            MedianAPM: 124,
                            KillFraction: 0.61,
                            detailedCount: 10,
                            Prestige: {
                                0: 0.2,
                                1: 0.3,
                                2: 0.1,
                                3: 0.4,
                            },
                            Mastery: {
                                0: 0.75,
                                1: 0.25,
                                2: 0.4,
                                3: 0.6,
                                4: 0.1,
                                5: 0.9,
                            },
                            MasteryDistribution: {
                                0: {
                                    0: 0.2,
                                    15: 0.3,
                                    30: 0.5,
                                },
                                1: {
                                    10: 0.6,
                                    20: 0.4,
                                },
                                2: {
                                    0: 0.9,
                                    30: 0.1,
                                },
                            },
                            MasteryDistributionByPrestige: {
                                0: {
                                    0: {
                                        0: 0.2,
                                        15: 0.3,
                                        30: 0.5,
                                    },
                                    1: {
                                        10: 0.6,
                                        20: 0.4,
                                    },
                                    2: {
                                        0: 0.9,
                                        30: 0.1,
                                    },
                                },
                                1: {
                                    0: {
                                        0: 0.25,
                                        30: 0.75,
                                    },
                                    1: {
                                        15: 1,
                                    },
                                    2: {
                                        30: 1,
                                    },
                                },
                                2: {
                                    0: {},
                                    1: {},
                                    2: {},
                                },
                                3: {
                                    0: {},
                                    1: {},
                                    2: {},
                                },
                            },
                            MasteryByPrestige: {
                                0: {
                                    0: 1,
                                    1: 0,
                                    2: 0.5,
                                    3: 0.5,
                                    4: 0,
                                    5: 1,
                                },
                                1: {
                                    0: 0.6,
                                    1: 0.4,
                                    2: 0.3,
                                    3: 0.7,
                                    4: 0.2,
                                    5: 0.8,
                                },
                                2: {
                                    0: 0.8,
                                    1: 0.2,
                                    2: 0.2,
                                    3: 0.8,
                                    4: 0.1,
                                    5: 0.9,
                                },
                                3: {
                                    0: 0.7,
                                    1: 0.3,
                                    2: 0.4,
                                    3: 0.6,
                                    4: 0.15,
                                    5: 0.85,
                                },
                            },
                        },
                        any: {
                            Frequency: 1,
                            Victory: 8,
                            Defeat: 2,
                            Winrate: 0.8,
                            MedianAPM: 124,
                            KillFraction: 0.61,
                            detailedCount: 10,
                        },
                    },
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
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Statistics" }).click();
        await page.getByRole("button", { name: "My Commanders" }).click();

        await expect(
            page.getByTestId("mastery-distribution-category"),
        ).toHaveCount(3);
        await expect(
            page.getByRole("row", {
                name: "Prestige 0 Prestige 1 Prestige 2 Prestige 3 Total",
            }),
        ).toBeVisible();
        await expect(
            page.getByRole("row", {
                name: "20% 30% 10% 40% 100%",
            }),
        ).toBeVisible();
        await expect(page.getByText("Toxic Nest Damage").first()).toBeVisible();
        await expect(
            page.getByText("Mend Healing Duration").first(),
        ).toBeVisible();
        const firstMasteryHeader = await page
            .getByTestId("mastery-distribution-header")
            .first()
            .evaluate((header) =>
                Array.from(header.children).map(
                    (child) => child.textContent || "",
                ),
            );
        expect(firstMasteryHeader).toEqual([
            "Mastery 1",
            "Choice 1 - Toxic Nest Damage",
            "Choice 2 - Mend Healing Duration",
        ]);
        await expect(page.getByTestId("mastery-distribution-line")).toHaveCount(
            12,
        );
        const masteryPrestigeListsFit = await page
            .getByTestId("mastery-distribution-prestige-list")
            .evaluateAll((lists) =>
                lists.every((list) => list.scrollWidth <= list.clientWidth + 1),
            );
        expect(masteryPrestigeListsFit).toBe(true);
        await expect(page.getByText("Prestige 0").first()).toBeVisible();
        await expect(page.getByText("Prestige 1").first()).toBeVisible();
        await expect(page.getByText("Mastery 1")).toBeVisible();
        await expect(
            page.getByLabel("Prestige 0 Choice 2: 20.0%"),
        ).toBeVisible();
        await expect(
            page.getByLabel("Prestige 0 Even: 30.0%").first(),
        ).toBeVisible();
        await expect(
            page.getByLabel("Prestige 1 Choice 1: 75.0%"),
        ).toBeVisible();
        await expect(
            page.getByTestId("mastery-distribution-even-line"),
        ).toHaveCount(12);
        await expect(
            page.getByTestId("mastery-distribution-point-label"),
        ).toHaveCount(10);
        const masteryPointLabelTexts = await page
            .getByTestId("mastery-distribution-point-label")
            .allTextContents();
        expect(masteryPointLabelTexts).toEqual(
            expect.arrayContaining([
                "20.0%",
                "30.0%",
                "50.0%",
                "60.0%",
                "75.0%",
                "100.0%",
            ]),
        );
        expect(
            masteryPointLabelTexts.filter((text) => text === "40.0%"),
        ).toHaveLength(0);
        await expect(
            page.getByText("Choice 2 leaning: 60.0%", { exact: true }),
        ).toHaveCount(0);
        await expect(
            page.getByText("Choice 1: 50.0%", { exact: true }),
        ).toHaveCount(0);
        await expect(
            page.getByText("Choice 2: 20.0%", { exact: true }),
        ).toHaveCount(0);
        await expect(
            page.getByText("Even: 30.0%", {
                exact: true,
            }),
        ).toHaveCount(0);
    });

    test("unit stats sum row preserves the stored wx total", async ({
        page,
    }) => {
        await installTauriMock(page, {
            status: "ok",
            stats: {
                ready: true,
                games: 9,
                detailed_parsed_count: 4,
                total_valid_files: 9,
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
                        main: {
                            Dehaka: {
                                count: 9,
                                sum: {
                                    created: 114,
                                    made: 1,
                                    lost: 5410,
                                    lost_percent: 47.45614035087719,
                                    kills: 16,
                                    KD: 0.002957486136783734,
                                    kill_percentage: 1,
                                },
                                "Brood Queen": {
                                    created: 4,
                                    made: 0.1,
                                    lost: 4,
                                    lost_percent: 1,
                                    kills: 1,
                                    KD: 0.25,
                                    kill_percentage: 0,
                                },
                                Dehaka: {
                                    created: 100,
                                    made: 1,
                                    lost: 5400,
                                    lost_percent: 54,
                                    kills: 10,
                                    KD: 0.001851851851851852,
                                    kill_percentage: 0.75,
                                },
                                "Primal Hydralisk": {
                                    created: 10,
                                    made: 0.8,
                                    lost: 6,
                                    lost_percent: 0.6,
                                    kills: 5,
                                    KD: 0.8333333333333334,
                                    kill_percentage: 0.25,
                                },
                                "Primal Drone": {
                                    created: 1000,
                                    made: 1,
                                    lost: 1000,
                                    lost_percent: 1,
                                    kills: 3,
                                    KD: 0.003,
                                    kill_percentage: 0,
                                },
                            },
                        },
                        ally: {},
                        amon: {},
                    },
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Statistics" }).click();
        await page.getByRole("button", { name: "Unit stats" }).click();
        await expect(
            page.getByText(
                "This tab only shows statistics from detailedly parsed replays. Detailedly parsed files: 4 / 9.",
            ),
        ).toBeVisible();

        await expect(
            page.getByText("Dehaka", { exact: true }).first(),
        ).toBeVisible();
        await expect(
            page.getByRole("cell", { name: "Brood Queen" }),
        ).toHaveCount(0);
        await expect(
            page
                .locator(".stats-unit-table-grid tbody tr.stats-sum-row td")
                .nth(3),
        ).toHaveText("5,410");
        await expect(
            page
                .locator(".stats-unit-table-grid tbody tr.stats-sum-row td")
                .nth(5),
        ).toHaveText("16");
    });

    test("unit and amon tabs show detailed parsed replay counts", async ({
        page,
    }) => {
        await installTauriMock(page, {
            status: "ok",
            stats: {
                ready: true,
                games: 5,
                detailed_parsed_count: 2,
                total_valid_files: 5,
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
                        main: {
                            Raynor: {
                                count: 2,
                                Marine: {
                                    created: 10,
                                    made: 1,
                                    lost: 3,
                                    lost_percent: 0.3,
                                    kills: 12,
                                    KD: 4,
                                    kill_percentage: 0.5,
                                },
                            },
                        },
                        ally: {},
                        amon: {
                            Zergling: {
                                created: 40,
                                lost: 40,
                                kills: 5,
                                KD: 0.1,
                            },
                        },
                    },
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Statistics" }).click();

        const detailMessage =
            "This tab only shows statistics from detailedly parsed replays. Detailedly parsed files: 2 / 5.";

        await page.getByRole("button", { name: "Unit stats" }).click();
        await expect(page.getByText(detailMessage)).toBeVisible();

        await page.getByRole("button", { name: "Amon stats" }).click();
        await expect(page.getByText(detailMessage)).toBeVisible();
    });

    test("Miner Evacuation fastest card matches the reference replay data", async ({
        page,
    }) => {
        await installTauriMock(page, {
            status: "ok",
            stats: {
                ready: true,
                games: 1,
                analysis_running: false,
                analysis_running_mode: null,
                message: "",
                query: "",
                main_handles: ["3-S2-1-900001"],
                commander_mastery: {
                    Abathur: [
                        "Toxic Nest Damage",
                        "Mend Healing Duration",
                        "Symbiote Ability Improvement",
                        "Double Biomass Chance",
                        "Toxic Nest Maximum Charges and Cooldown",
                        "Structure Morph and Evolution Rate",
                    ],
                    Swann: [
                        "Concentrated Beam Width and Damage",
                        "Combat Drop Duration and Life",
                        "Immortality Protocol Cost and Build Time",
                        "Structure Health",
                        "Vespene Drone Cost",
                        "Laser Drill Build Time, Upgrade Time, and Upgrade Cost",
                    ],
                },
                prestige_names: {
                    Abathur: {
                        en: ["Evolution Master"],
                        ko: ["진화 군주"],
                    },
                    Swann: {
                        en: ["Chief Engineer"],
                        ko: ["수석 기술자"],
                    },
                },
                analysis: {
                    MapData: {
                        "Miner Evacuation": {
                            average_victory_time: 1041.75,
                            frequency: 1,
                            Victory: 1,
                            Defeat: 0,
                            Winrate: 1,
                            bonus: 0,
                            Fastest: {
                                length: 1041.75,
                                file: "fixtures/accounts/slot-main/replays/miner-evacuation.SC2Replay",
                                date: 1538345544,
                                difficulty: "Normal",
                                enemy_race: "테란",
                                players: [
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
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Statistics" }).click();

        await expect(
            page.locator(".stats-map-players .stats-map-player h4").nth(0),
        ).toHaveText("Main Tester");
        await expect(
            page.locator(".stats-map-players .stats-map-player h4").nth(1),
        ).toHaveText("Partner Tester");
        await expect(page.locator(".stats-map-sub")).toHaveText(
            "17:21 | Terran",
        );
        await expect(page.locator(".stats-map-foot")).toContainText(
            "Normal | 2018-09-30 22:12:24",
        );
        await expect(page.locator(".stats-map-player").nth(0)).toContainText(
            "Evolution Master (P0)",
        );
        await expect(page.locator(".stats-map-player").nth(1)).toContainText(
            "Chief Engineer (P0)",
        );
        await expect(page.locator(".stats-map-player").nth(0)).toContainText(
            "0 Toxic Nest Damage",
        );
        await expect(page.locator(".stats-map-player").nth(0)).toContainText(
            "0 Structure Morph and Evolution Rate",
        );
        await expect(page.locator(".stats-map-player").nth(1)).toContainText(
            "0 Combat Drop Duration and Life",
        );
        await expect(page.locator(".stats-map-player").nth(1)).toContainText(
            "0 Laser Drill Build Time, Upgrade Time, and Upgrade Cost",
        );
        await expect(page.locator(".stats-map-player-empty")).toHaveCount(0);
    });

    test("statistics checkbox filters refresh immediately", async ({
        page,
    }) => {
        await installTauriMock(
            page,
            {
                status: "ok",
                stats: {
                    ready: true,
                    games: 10,
                    analysis_running: false,
                    analysis_running_mode: null,
                    message: "",
                    query: "",
                    analysis: {
                        MapData: {},
                        CommanderData: {},
                        AllyCommanderData: {},
                        DifficultyData: {
                            Brutal: { Victory: 6, Defeat: 0, Winrate: 1 },
                            Normal: { Victory: 4, Defeat: 0, Winrate: 1 },
                        },
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
            },
            [
                {
                    match: "difficulty_filter=Brutal",
                    response: {
                        status: "ok",
                        stats: {
                            ready: true,
                            games: 4,
                            analysis_running: false,
                            analysis_running_mode: null,
                            message: "",
                            query: "difficulty_filter=Brutal",
                            analysis: {
                                MapData: {},
                                CommanderData: {},
                                AllyCommanderData: {},
                                DifficultyData: {
                                    Normal: {
                                        Victory: 4,
                                        Defeat: 0,
                                        Winrate: 1,
                                    },
                                },
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
                    },
                },
            ],
        );

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Statistics" }).click();

        await expect(
            page.getByText("Games found: 10", { exact: true }),
        ).toBeVisible();
        await page
            .getByRole("checkbox", { name: "Brutal", exact: true })
            .click();
        await expect(
            page.getByText("Games found: 4", { exact: true }),
        ).toBeVisible({
            timeout: 200,
        });
    });

    test("statistics abnormal mastery filter refreshes immediately", async ({
        page,
    }) => {
        await installTauriMock(
            page,
            {
                status: "ok",
                stats: {
                    ready: true,
                    games: 10,
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
            },
            [
                {
                    match: "main_normal_mastery=0",
                    response: {
                        status: "ok",
                        stats: {
                            ready: true,
                            games: 2,
                            analysis_running: false,
                            analysis_running_mode: null,
                            message: "",
                            query: "main_normal_mastery=0",
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
                    },
                },
            ],
        );

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Statistics" }).click();

        await expect(
            page.getByText("Games found: 10", { exact: true }),
        ).toBeVisible();
        await page
            .locator(".stats-filter-group")
            .filter({ hasText: "Main mastery point" })
            .getByRole("checkbox", {
                name: "Mastery Point <= 90",
                exact: true,
            })
            .click();
        await expect(
            page.getByText("Games found: 2", { exact: true }),
        ).toBeVisible({
            timeout: 200,
        });
    });

    test("detailed analysis disables simple-analysis and delete buttons", async ({
        page,
    }) => {
        await installTauriMock(page, {
            status: "ok",
            stats: {
                ready: false,
                games: 0,
                analysis_running: true,
                analysis_running_mode: "detailed",
                detailed_analysis_status:
                    "Detailed analysis: generating cache.",
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
                    UnitData: null,
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });

        await page.getByRole("button", { name: "Statistics" }).click();
        await expect(
            page.getByRole("button", { name: "Run simple analysis" }),
        ).toBeDisabled();

        await page.getByRole("button", { name: "Settings" }).click();
        await expect(
            page.getByRole("button", { name: "Delete parsed data" }),
        ).toBeDisabled();
    });

    test("renders the legacy shell on /#/config", async ({ page }) => {
        await installTauriMock(page);
        await page.goto("/", { waitUntil: "domcontentloaded" });

        await expect(
            page.getByRole("heading", { name: "SCO Overlay Config" }),
        ).toBeVisible();
        await expect(page.locator("#app-status")).toBeVisible();
        await expect(page.locator("#app-tab-nav")).toBeVisible();
        await expect(page.locator("#app-content")).toBeVisible();
        await expect(page.locator("#app-footer")).toHaveCount(1);
        await expect(page.locator("#app-save")).toHaveCount(1);
        await expect(page.locator("#app-revert")).toHaveCount(1);
        await expect(page.locator("#app-reload")).toHaveCount(1);
        await expect(page.locator("#app-footer")).toHaveClass(/is-hidden/);
        await expect(page.locator("#app-tab-nav .tab-btn")).toHaveCount(8);
    });

    test("monitor selector uses indexed monitor names from the backend", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                monitor: 2,
            },
            monitorCatalog: [
                { index: 1, label: "1 - ASUS VG27A" },
                { index: 2, label: "2 - LG C2" },
            ],
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });

        const monitorSelect = page
            .locator("label.main-number-row")
            .filter({ hasText: "Monitor" })
            .locator("select.input");
        await expect(monitorSelect).toHaveValue("2");
        await expect(monitorSelect.locator("option")).toHaveText([
            "1 - ASUS VG27A",
            "2 - LG C2",
        ]);
    });

    test("settings tab keeps the supported toggles and hides removed overlay-only settings", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                enable_logging: true,
                show_session: true,
                show_player_winrates: true,
                show_replay_info_after_game: true,
                fast_expand: true,
                force_hide_overlay: true,
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });

        await expect(
            page.getByRole("checkbox", { name: "Enable logging" }),
        ).toBeVisible();
        await expect(
            page.getByRole("checkbox", { name: "Show session stats" }),
        ).toBeVisible();
        await expect(
            page.getByRole("checkbox", {
                name: "Show player winrates and notes",
            }),
        ).toBeVisible();
        await expect(
            page.getByRole("checkbox", { name: "Show replay info after game" }),
        ).toBeVisible();
        await expect(
            page.getByRole("checkbox", { name: "Fast expand hints" }),
        ).toHaveCount(0);
        await expect(
            page.getByRole("checkbox", {
                name: "Don't show overlay on-screen",
            }),
        ).toHaveCount(0);
    });

    test("start minimized depends on minimize to tray", async ({ page }) => {
        await installTauriMock(page, null, [], {
            settings: {
                minimize_to_tray: false,
                start_minimized: true,
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });

        const minimizeToTray = page.getByRole("checkbox", {
            name: "Minimize to tray",
        });
        const startMinimized = page.getByRole("checkbox", {
            name: "Start minimized",
        });

        await expect(minimizeToTray).not.toBeChecked();
        await expect(startMinimized).toBeDisabled();
        await expect(startMinimized).toBeChecked();

        await minimizeToTray.click();
        await expect(startMinimized).toBeEnabled();
        await expect(startMinimized).toBeChecked();

        await minimizeToTray.click();
        await expect(startMinimized).toBeDisabled();
        await expect(startMinimized).toBeChecked();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_APPLY_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                minimize_to_tray: false,
                start_minimized: true,
            });
    });

    test("missing tray setting defaults start minimized to disabled", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {},
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });

        const minimizeToTray = page.getByRole("checkbox", {
            name: "Minimize to tray",
        });
        const startMinimized = page.getByRole("checkbox", {
            name: "Start minimized",
        });

        await expect(minimizeToTray).not.toBeChecked();
        await expect(startMinimized).toBeDisabled();
        await expect(startMinimized).not.toBeChecked();
    });

    test("save and revert stay disabled until settings change", async ({
        page,
    }) => {
        await installTauriMock(page);
        await page.goto("/", { waitUntil: "domcontentloaded" });

        const saveButton = page.getByRole("button", { name: /^Save$/ });
        const revertButton = page.getByRole("button", { name: /^Revert$/ });
        const showCharts = page.getByRole("checkbox", { name: "Show charts" });

        await expect(saveButton).toBeDisabled();
        await expect(revertButton).toBeDisabled();

        await showCharts.uncheck();

        await expect(saveButton).toBeEnabled();
        await expect(revertButton).toBeEnabled();

        await revertButton.click();

        await expect(showCharts).toBeChecked();
        await expect(saveButton).toBeDisabled();
        await expect(revertButton).toBeDisabled();
    });

    test("path buttons apply immediately and save only after Save", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                account_folder: "fixtures/accounts",
                screenshot_folder: "fixtures/screenshots",
            },
            folderPickerResponses: {
                "fixtures/accounts": "fixtures/accounts-updated",
                "fixtures/screenshots": "fixtures/screenshots-updated",
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });

        await page
            .getByRole("button", { name: "Account folder", exact: true })
            .click();
        await page
            .getByRole("button", { name: "Screenshot folder", exact: true })
            .click();

        await expect(
            page.getByText("fixtures/accounts-updated", { exact: true }),
        ).toBeVisible();
        await expect(
            page.getByText("fixtures/screenshots-updated", { exact: true }),
        ).toBeVisible();
        await expect(
            page.getByText(
                "Folder selected and applied. Click Save to persist.",
            ),
        ).toBeVisible();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_FOLDER_PICKER_REQUESTS__"] || [];
                    return requests;
                }),
            )
            .toEqual([
                {
                    title: "Account folder path",
                    directory: "fixtures/accounts",
                },
                {
                    title: "Screenshot folder path",
                    directory: "fixtures/screenshots",
                },
            ]);

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_APPLY_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                account_folder: "fixtures/accounts-updated",
                screenshot_folder: "fixtures/screenshots-updated",
            });

        await page.getByRole("button", { name: /^Save$/ }).click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_SAVE_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                account_folder: "fixtures/accounts-updated",
                screenshot_folder: "fixtures/screenshots-updated",
            });
    });

    test("overlay colors use an inline picker and save the selected value", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                color_player1: "#112233",
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });

        await page
            .locator(".color-row")
            .filter({ hasText: /^Player 1$/ })
            .getByRole("button")
            .click();

        const player1Color = page.locator(
            'input[aria-label="Player 1 color value"]',
        );
        await expect(player1Color).toHaveValue("#112233");

        await player1Color.fill("#445566");
        await player1Color.press("Enter");

        await expect(player1Color).toHaveValue("#445566");

        await page.getByRole("button", { name: /^Save$/ }).click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_SAVE_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                color_player1: "#445566",
            });
    });

    test("recording a hotkey removes its trigger until capture ends", async ({
        page,
    }) => {
        await installTauriMock(page);
        await page.goto("/", { waitUntil: "domcontentloaded" });

        const hotkeyInput = page
            .locator(".hotkey-entry")
            .filter({ has: page.getByRole("button", { name: "Show / Hide" }) })
            .locator("input.hotkey-input");

        await hotkeyInput.click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests = window["__SCO_ACTION_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                action: "hotkey_reassign_begin",
                path: "hotkey_show/hide",
            });

        await page.getByRole("heading", { name: "SCO Overlay Config" }).click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests = window["__SCO_ACTION_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                action: "hotkey_reassign_end",
                path: "hotkey_show/hide",
            });
    });

    test("players tab formats last seen as datetime", async ({ page }) => {
        await installTauriMock(page, null, [], {
            tabResponses: {
                players: {
                    status: "ok",
                    players: [
                        {
                            player: "AllyPlayer",
                            wins: 3,
                            losses: 1,
                            winrate: 0.75,
                            apm: 123,
                            commander: "Abathur",
                            kills: 0.41,
                            last_seen: 1538345544,
                        },
                    ],
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Players" }).click();

        await expect(
            page.locator("table.data-table tbody tr").nth(0),
        ).toContainText("2018-09-30 22:12:24");
    });

    test("players tab defaults to last seen descending", async ({ page }) => {
        await installTauriMock(page, null, [], {
            tabResponses: {
                players: {
                    status: "ok",
                    players: [
                        {
                            player: "OlderPlayer",
                            wins: 1,
                            losses: 0,
                            winrate: 1,
                            apm: 80,
                            commander: "Karax",
                            kills: 0.2,
                            last_seen: 1538345544,
                        },
                        {
                            player: "NewerPlayer",
                            wins: 2,
                            losses: 1,
                            winrate: 0.66,
                            apm: 140,
                            commander: "Abathur",
                            kills: 0.5,
                            last_seen: 1735689600,
                        },
                    ],
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Players" }).click();

        await expect(
            page.locator("table.data-table tbody tr").nth(0),
        ).toContainText("NewerPlayer");
        await expect(
            page.locator("table.data-table tbody tr").nth(1),
        ).toContainText("OlderPlayer");
    });

    test("reassigned hotkeys apply immediately and save only after Save is pressed", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                "hotkey_show/hide": "Ctrl+Shift+*",
            },
        });
        await page.goto("/", { waitUntil: "domcontentloaded" });

        const hotkeyInput = page
            .locator(".hotkey-entry")
            .filter({
                has: page.getByRole("button", { name: /^Show \/ Hide$/ }),
            })
            .locator("input.hotkey-input");

        await hotkeyInput.click();
        await hotkeyInput.dispatchEvent("keydown", {
            key: "Control",
            code: "ControlLeft",
            ctrlKey: true,
        });

        await expect(hotkeyInput).toHaveClass(/is-recording/);

        await hotkeyInput.dispatchEvent("keydown", {
            key: "P",
            code: "KeyP",
            ctrlKey: true,
            shiftKey: true,
        });

        await expect(hotkeyInput).not.toHaveClass(/is-recording/);
        await expect(hotkeyInput).toHaveValue("Ctrl+Shift+P");

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_APPLY_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                "hotkey_show/hide": "Ctrl+Shift+P",
            });

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests = window["__SCO_ACTION_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                action: "hotkey_reassign_end",
                path: "hotkey_show/hide",
            });

        await page.getByRole("button", { name: /^Save$/ }).click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_SAVE_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                "hotkey_show/hide": "Ctrl+Shift+P",
            });
    });

    test("shifted symbol keys stay normalized in draft and save as base keys", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                "hotkey_show/hide": "Ctrl+Shift+8",
            },
        });
        await page.goto("/", { waitUntil: "domcontentloaded" });

        const hotkeyInput = page
            .locator(".hotkey-entry")
            .filter({
                has: page.getByRole("button", { name: /^Show \/ Hide$/ }),
            })
            .locator("input.hotkey-input");

        await hotkeyInput.click();
        await expect(hotkeyInput).toHaveClass(/is-recording/);

        await hotkeyInput.dispatchEvent("keydown", {
            key: "&",
            code: "Digit7",
            ctrlKey: true,
            shiftKey: true,
        });

        await expect(hotkeyInput).toHaveValue("Ctrl+Shift+7");
        await expect(hotkeyInput).not.toHaveClass(/is-recording/);

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_APPLY_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                "hotkey_show/hide": "Ctrl+Shift+7",
            });

        await page.getByRole("button", { name: /^Save$/ }).click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests =
                        window["__SCO_CONFIG_SAVE_REQUESTS__"] || [];
                    return requests[requests.length - 1] || null;
                }),
            )
            .toMatchObject({
                "hotkey_show/hide": "Ctrl+Shift+7",
            });
    });

    test("escape and backspace clear hotkey assignments in draft and Revert restores saved values", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            settings: {
                "hotkey_show/hide": "Ctrl+Shift+*",
                hotkey_show: "Ctrl+Alt+K",
            },
        });
        await page.goto("/", { waitUntil: "domcontentloaded" });

        const showHideInput = page
            .locator(".hotkey-entry")
            .filter({
                has: page.getByRole("button", { name: /^Show \/ Hide$/ }),
            })
            .locator("input.hotkey-input");
        const showInput = page
            .locator(".hotkey-entry")
            .filter({ has: page.getByRole("button", { name: /^Show$/ }) })
            .locator("input.hotkey-input");

        await showHideInput.click();
        await expect(showHideInput).toHaveClass(/is-recording/);
        await showHideInput.dispatchEvent("keydown", {
            key: "Escape",
            code: "Escape",
        });

        await expect(showHideInput).toHaveValue("");
        await expect(showHideInput).not.toHaveClass(/is-recording/);

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await showInput.click();
        await expect(showInput).toHaveClass(/is-recording/);
        await showInput.dispatchEvent("keydown", {
            key: "Backspace",
            code: "Backspace",
        });

        await expect(showInput).toHaveValue("");
        await expect(showInput).not.toHaveClass(/is-recording/);

        await expect
            .poll(() =>
                page.evaluate(
                    () => (window["__SCO_CONFIG_SAVE_REQUESTS__"] || []).length,
                ),
            )
            .toBe(0);

        await page
            .locator(".main-bottom-right button")
            .filter({ hasText: /^Revert$/ })
            .click();

        await expect(showHideInput).toHaveValue("Ctrl+Shift+*");
        await expect(showInput).toHaveValue("Ctrl+Alt+K");
    });

    test("loads with hash route fallback", async ({ page }) => {
        await page.goto("/", { waitUntil: "domcontentloaded" });
        await expect(page).toHaveURL(/#\/config$/);
    });

    test("randomizer tab matches the legacy selection and result flow", async ({
        page,
    }) => {
        await installTauriMock(page);
        await page.goto("/", { waitUntil: "domcontentloaded" });

        await page.getByRole("button", { name: "Randomizer" }).click();

        await expect(
            page.getByText("Commander and prestige choices"),
        ).toBeVisible();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P0" }),
        ).toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P1" }),
        ).not.toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Fenix P0" }),
        ).toBeChecked();
        await page
            .getByRole("button", { name: "Toggle all prestiges for Abathur" })
            .click();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P0" }),
        ).toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P1" }),
        ).toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P2" }),
        ).toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P3" }),
        ).toBeChecked();
        await page
            .getByRole("button", { name: "Toggle P0 for all commanders" })
            .click();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P0" }),
        ).not.toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Fenix P0" }),
        ).not.toBeChecked();
        await page.getByRole("button", { name: "Generate" }).click();

        await expect(
            page.getByText("Fenix - Purifier Executor (P0)"),
        ).toBeVisible();
        await expect(
            page.getByText("30 Champion A.I. Attack Speed"),
        ).toBeVisible();
        await expect(page.getByText("Scythe of Amon | Zerg")).toBeVisible();
    });
});
