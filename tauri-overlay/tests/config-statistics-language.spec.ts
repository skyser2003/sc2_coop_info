import { expect, test } from "@playwright/test";

test.setTimeout(60000);

async function installStatisticsLanguageMock(page) {
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

test("statistics subtabs are localized", async ({ page }) => {
    await installStatisticsLanguageMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.getByRole("tab", { name: "Statistics" }).click();
    await expect(page.locator("nav").last().getByRole("button")).toHaveText([
        "Maps",
        "Allied commanders",
        "My commanders",
        "Difficulty and regions",
        "Unit stats",
        "Amon stats",
    ]);

    await page.getByRole("tab", { name: "Settings" }).click();
    await expect(
        page.getByRole("heading", { name: "Detailed analysis" }),
    ).toBeVisible();
    await expect(
        page.getByRole("checkbox", {
            name: "Perform detailed analysis at start",
        }),
    ).toBeVisible();
    const languageSelect = page.locator("select").first();
    await languageSelect.selectOption("ko");

    await page.getByRole("tab", { name: "통계" }).click();
    await expect(page.locator("nav").last().getByRole("button")).toHaveText([
        "맵",
        "동맹 사령관",
        "내 사령관",
        "난이도 및 지역",
        "유닛 통계",
        "아몬 통계",
    ]);

    await page.getByRole("tab", { name: "설정" }).click();
    await expect(
        page.getByRole("heading", { name: "상세 분석" }),
    ).toBeVisible();
    await expect(
        page.getByRole("checkbox", { name: "시작 시 상세 분석 수행" }),
    ).toBeVisible();
});
