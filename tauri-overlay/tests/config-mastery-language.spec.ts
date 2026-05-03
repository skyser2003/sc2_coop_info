import { expect, test } from "@playwright/test";

test.setTimeout(60000);

async function installMasteryLanguageMock(page) {
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

        const commanderMastery = {
            Abathur: {
                en: [
                    "Toxic Nest Damage",
                    "Mend Healing Duration",
                    "Symbiote Ability Improvement",
                    "Double Biomass Chance",
                    "Toxic Nest Maximum Charges and Cooldown",
                    "Structure Morph and Evolution Rate",
                ],
                ko: [
                    "독성 둥지 공격력",
                    "치유 회복 지속 시간",
                    "공생체 능력 향상",
                    "생체 물질 확률 2배",
                    "독성 둥지 최대 충전 및 재사용 대기시간",
                    "구조물 변이 및 진화 속도",
                ],
            },
            Fenix: {
                en: [
                    "Fenix Suit Attack Speed",
                    "Fenix Suit Offline Energy Regeneration",
                    "Champion A.I. Attack Speed",
                    "Champion A.I. Life and Shields",
                    "Chrono Boost Efficiency",
                    "Extra Starting Supply",
                ],
                ko: [
                    "피닉스 전투복 공격 속도",
                    "피닉스 전투복 오프라인 에너지 재생률",
                    "용사 인공지능 공격 속도",
                    "용사 인공지능 체력 및 보호막",
                    "시간 증폭 효율",
                    "추가 시작 보급품",
                ],
            },
        };
        const prestigeNames = {
            Abathur: {
                en: ["Evolution Master"],
                ko: ["진화 군주"],
            },
            Fenix: {
                en: ["Purifier Executor"],
                ko: ["정화자 집행관"],
            },
        };
        const randomizerCatalog = {
            commander_mastery: commanderMastery,
            prestige_names: prestigeNames,
            mutators: [],
            brutal_plus: [],
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
            commander_mastery: commanderMastery,
            prestige_names: {
                Abathur: {
                    en: ["Evolution Master"],
                    ko: ["진화 군주"],
                },
            },
            main_handles: ["3-S2-1-900001"],
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
                            enemy_race: "Terran",
                            players: [
                                {
                                    name: "Tester",
                                    handle: "3-S2-1-900001",
                                    commander: "Abathur",
                                    apm: 123,
                                    mastery_level: 90,
                                    masteries: [30, 0, 30, 0, 30, 0],
                                    prestige: 0,
                                    prestige_name: "Evolution Master",
                                },
                                {
                                    name: "Partner",
                                    handle: "3-S2-1-900002",
                                    commander: "Fenix",
                                    apm: 88,
                                    mastery_level: 90,
                                    masteries: [0, 30, 0, 30, 0, 30],
                                    prestige: 0,
                                    prestige_name: "Purifier Executor",
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
        const randomizerGeneratePayload = () => ({
            status: "ok",
            result: { ok: true, path: null },
            message: "Generated random commander",
            randomizer: {
                kind: "commander",
                commander: "Fenix",
                prestige: 0,
                mastery_indices: [30, 30, 30],
                map_race: "Scythe of Amon | Zerg",
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
                    if (request?.action === "randomizer_generate") {
                        return randomizerGeneratePayload();
                    }
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
                    if (request.body?.action === "randomizer_generate") {
                        return randomizerGeneratePayload();
                    }

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

test("localized commander mastery data drives statistics and randomizer labels", async ({
    page,
}) => {
    await installMasteryLanguageMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.getByRole("tab", { name: "Statistics" }).click();
    await page.locator("tbody tr").first().click();
    await expect(page.getByText("30 Toxic Nest Damage")).toBeVisible();

    await page.getByRole("tab", { name: "Settings" }).click();
    const languageSelect = page.locator("select").first();
    await languageSelect.selectOption("ko");

    await page.getByRole("tab", { name: "통계" }).click();
    await page.locator("tbody tr").first().click();
    await expect(page.getByText("30 독성 둥지 공격력")).toBeVisible();

    await page.getByRole("tab", { name: "랜덤 선택" }).click();
    await page.getByRole("button", { name: "생성" }).first().click();
    await expect(page.getByText("30 피닉스 전투복 공격 속도")).toBeVisible();
});
