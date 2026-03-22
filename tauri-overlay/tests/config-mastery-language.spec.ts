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
                            commander_mastery: commanderMastery,
                            prestige_names: {
                                Abathur: {
                                    en: ["Evolution Master"],
                                    ko: ["진화 군주"],
                                },
                                Fenix: {
                                    en: ["Purifier Executor"],
                                    ko: ["정화자 집행관"],
                                },
                            },
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
                            commander_mastery: commanderMastery,
                            prestige_names: {
                                Abathur: {
                                    en: ["Evolution Master"],
                                    ko: ["진화 군주"],
                                },
                                Fenix: {
                                    en: ["Purifier Executor"],
                                    ko: ["정화자 집행관"],
                                },
                            },
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
                            simple_analysis_running: false,
                            detailed_analysis_running: false,
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
                                                    masteries: [
                                                        30, 0, 30, 0, 30, 0,
                                                    ],
                                                    prestige: 0,
                                                    prestige_name:
                                                        "Evolution Master",
                                                },
                                                {
                                                    name: "Partner",
                                                    handle: "3-S2-1-900002",
                                                    commander: "Fenix",
                                                    apm: 88,
                                                    mastery_level: 90,
                                                    masteries: [
                                                        0, 30, 0, 30, 0, 30,
                                                    ],
                                                    prestige: 0,
                                                    prestige_name:
                                                        "Purifier Executor",
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
                    if (request.body?.action === "randomizer_generate") {
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
                                        points: 30,
                                        label: "Fenix Suit Attack Speed",
                                    },
                                    {
                                        points: 0,
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

                throw new Error(`Unexpected request: ${method} ${path}`);
            },
            event: {
                listen: async () => () => {},
            },
        };
    });
}

test("localized commander mastery data drives statistics and randomizer labels", async ({
    page,
}) => {
    await installMasteryLanguageMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.getByRole("button", { name: "Statistics" }).click();
    await page.locator("table.data-table tbody tr").first().click();
    await expect(page.getByText("30 Toxic Nest Damage")).toBeVisible();

    await page.getByRole("button", { name: "Settings" }).click();
    const languageSelect = page
        .locator(".main-settings-inline-numbers select")
        .first();
    await languageSelect.selectOption("ko");

    await page.getByRole("button", { name: "통계" }).click();
    await page.locator("table.data-table tbody tr").first().click();
    await expect(page.getByText("30 독성 둥지 공격력")).toBeVisible();

    await page.getByRole("button", { name: "랜덤 선택" }).click();
    await page.getByRole("button", { name: "Generate" }).click();
    await expect(page.getByText("30 피닉스 전투복 공격 속도")).toBeVisible();
});
