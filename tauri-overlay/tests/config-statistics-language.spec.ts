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
                            simple_analysis_running: false,
                            detailed_analysis_running: false,
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

test("statistics subtabs are localized", async ({ page }) => {
    await installStatisticsLanguageMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.getByRole("button", { name: "Statistics" }).click();
    await expect(page.locator(".stats-subtabs button")).toHaveText([
        "Maps",
        "Allied commanders",
        "My commanders",
        "Difficulty and regions",
        "Unit stats",
        "Amon stats",
    ]);

    await page.getByRole("button", { name: "Settings" }).click();
    await expect(
        page.getByRole("heading", { name: "Detailed analysis" }),
    ).toBeVisible();
    await expect(
        page.getByRole("checkbox", {
            name: "Perform detailed analysis at start",
        }),
    ).toBeVisible();
    const languageSelect = page
        .locator(".main-settings-inline-numbers select")
        .first();
    await languageSelect.selectOption("ko");

    await page.getByRole("button", { name: "통계" }).click();
    await expect(page.locator(".stats-subtabs button")).toHaveText([
        "맵",
        "동맹 사령관",
        "내 사령관",
        "난이도 및 지역",
        "유닛 통계",
        "아몬 통계",
    ]);

    await page.getByRole("button", { name: "설정" }).click();
    await expect(
        page.getByRole("heading", { name: "상세 분석" }),
    ).toBeVisible();
    await expect(
        page.getByRole("checkbox", { name: "시작 시 상세 분석 수행" }),
    ).toBeVisible();
});
