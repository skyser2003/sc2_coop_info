import { expect, test, type Page } from "@playwright/test";

type ConfigSettings = {
    dark_theme?: boolean;
    account_folder?: string;
    main_names?: string[];
    detailed_analysis_atstart?: boolean;
    rng_choices?: Record<string, string>;
    minimize_to_tray?: boolean;
    start_minimized?: boolean;
};

type ThemeCommanderStats = {
    Frequency: number;
    Victory: number;
    Defeat: number;
    Winrate: number;
    MedianAPM: number;
    KillFraction: number;
    detailedCount: number;
    Prestige?: Record<string, number>;
    MasteryDistributionByPrestige?: Record<
        string,
        Record<string, Record<string, number>>
    >;
};

type ThemeStatsAnalysis = {
    MapData: Record<string, never>;
    CommanderData: Record<string, ThemeCommanderStats>;
    AllyCommanderData: Record<string, never>;
    DifficultyData: Record<string, never>;
    RegionData: Record<string, never>;
    PlayerData: Record<string, never>;
    AmonData: Record<string, never>;
    MapDataReady: boolean;
    UnitData: {
        main: Record<string, never>;
        ally: Record<string, never>;
        amon: Record<string, never>;
    };
};

type ConfigStatsPayload = {
    status: "ok";
    ready: boolean;
    games: number;
    analysis_running: boolean;
    analysis_running_mode: string | null;
    message: string;
    query?: string;
    analysis?: ThemeStatsAnalysis;
};

const LIGHT_THEME_READY_STATS: ConfigStatsPayload = {
    status: "ok",
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
                MasteryDistributionByPrestige: {
                    0: {
                        0: {
                            0: 0.2,
                            50: 0.3,
                            100: 0.5,
                        },
                    },
                    1: {
                        0: {
                            50: 1,
                        },
                    },
                    2: {
                        0: {},
                    },
                    3: {
                        0: {},
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
};

async function installThemeMock(
    page: Page,
    settingsOverride: ConfigSettings = {},
    statsOverride: ConfigStatsPayload | null = null,
) {
    await page.addInitScript(
        ({ providedSettings, providedStats }) => {
            const cloneJson = <T>(value: T): T =>
                JSON.parse(JSON.stringify(value)) as T;

            let settings = {
                dark_theme: true,
                account_folder: "fixtures/accounts",
                main_names: [],
                detailed_analysis_atstart: false,
                rng_choices: {},
                minimize_to_tray: true,
                start_minimized: true,
                ...providedSettings,
            };
            let activeSettings = cloneJson(settings);
            const randomizerCatalog = {
                commander_mastery: {
                    Abathur: [
                        "Toxic Nest Damage",
                        "Mend Healing Duration",
                        "Symbiote Ability Improvement",
                        "Double Biomass Chance",
                        "Toxic Nest Maximum Charges and Cooldown",
                        "Structure Morph and Evolution Rate",
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
                },
                mutators: [],
                brutal_plus: [],
            };
            const configPayload = () => ({
                status: "ok",
                settings,
                active_settings: activeSettings,
                randomizer_catalog: randomizerCatalog,
                monitor_catalog: [{ index: 1, label: "1 - Primary Monitor" }],
            });
            const weekliesPayload = () => ({
                status: "ok",
                weeklies: [
                    {
                        mutation: "Distant Threat",
                        nameEn: "Distant Threat",
                        map: "Dead of Night",
                        isCurrent: true,
                        nextDuration: "Now",
                        nextDurationDays: 0,
                        difficulty: "Brutal",
                        wins: 1,
                        losses: 0,
                        winrate: 1,
                        mutators: [],
                    },
                ],
            });
            const playersPayload = () => ({
                status: "ok",
                players: [
                    {
                        handle: "3-S2-1-900001",
                        player: "Main Tester",
                        player_names: ["Main Tester", "Main Alias"],
                        wins: 1599,
                        losses: 331,
                        winrate: 0.828,
                        apm: 123,
                        commander: "Abathur",
                        kills: 0.41,
                        last_seen: 1735689600,
                    },
                ],
                total_players: 1,
                loading: false,
            });
            const statsPayload = (): ConfigStatsPayload => ({
                status: "ok",
                ready: false,
                games: 0,
                analysis_running: false,
                analysis_running_mode: null,
                message: "",
                ...(providedStats ?? {}),
            });

            window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener: () => {},
            };

            (
                window as typeof window & {
                    __TAURI_INTERNALS__?: {
                        invoke: (
                            command: string,
                            request: {
                                path?: string;
                                method?: string;
                                settings?: typeof settings;
                                persist?: boolean;
                                body?: {
                                    settings?: typeof settings;
                                    persist?: boolean;
                                };
                            },
                        ) => Promise<unknown>;
                        event: {
                            listen: () => Promise<() => void>;
                        };
                        transformCallback: (callback: () => void) => number;
                    };
                }
            ).__TAURI_INTERNALS__ = {
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
                        const nextSettings = request.settings ?? activeSettings;
                        if (request.persist === false) {
                            activeSettings = cloneJson(nextSettings);
                        } else {
                            settings = cloneJson(nextSettings);
                            activeSettings = cloneJson(nextSettings);
                        }
                        return configPayload();
                    }
                    if (command === "config_weeklies_get") {
                        return weekliesPayload();
                    }
                    if (command === "config_players_get") {
                        return playersPayload();
                    }
                    if (command === "config_replays_get") {
                        return {
                            status: "ok",
                            replays: [],
                            total_replays: 0,
                            selected_replay_file: "",
                        };
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
                    if (command !== "config_request") {
                        throw new Error(`Unexpected command: ${command}`);
                    }

                    if (
                        request.method === "GET" &&
                        request.path === "/config"
                    ) {
                        return configPayload();
                    }

                    if (
                        request.method === "POST" &&
                        request.path === "/config"
                    ) {
                        const nextSettings =
                            request.body?.settings ?? activeSettings;
                        if (request.body?.persist === false) {
                            activeSettings = cloneJson(nextSettings);
                        } else {
                            settings = cloneJson(nextSettings);
                            activeSettings = cloneJson(nextSettings);
                        }

                        return configPayload();
                    }

                    if (
                        request.method === "POST" &&
                        request.path === "/config/stats/action"
                    ) {
                        return { status: "ok", message: "ok" };
                    }

                    if (
                        request.method === "GET" &&
                        request.path === "/config/weeklies"
                    ) {
                        return weekliesPayload();
                    }

                    if (
                        request.method === "GET" &&
                        request.path.startsWith("/config/players?")
                    ) {
                        return playersPayload();
                    }

                    if (
                        request.method === "GET" &&
                        request.path.startsWith("/config/stats?")
                    ) {
                        return {
                            status: "ok",
                            stats: statsPayload(),
                        };
                    }

                    throw new Error(
                        `Unexpected request: ${request.method} ${request.path}`,
                    );
                },
                event: {
                    listen: async () => () => {},
                },
                transformCallback: (callback: () => void) => {
                    const id = Math.floor(Math.random() * 1000000);
                    window[`_${id}`] = callback;
                    return id;
                },
            };
        },
        { providedSettings: settingsOverride, providedStats: statsOverride },
    );
}

test("dark theme setting switches the config page into light mode live", async ({
    page,
}) => {
    await installThemeMock(page, { dark_theme: true });

    await page.goto("/", { waitUntil: "domcontentloaded" });

    const pageBody = page.locator("body");
    const darkThemeToggle = page.getByRole("checkbox", { name: "Dark theme" });

    await expect(pageBody).toHaveCSS("background-color", "rgb(11, 18, 32)");
    await expect(pageBody).toHaveCSS("color", "rgb(229, 231, 235)");
    await expect(darkThemeToggle).toBeChecked();

    await darkThemeToggle.uncheck();

    await expect(pageBody).toHaveCSS("background-color", "rgb(243, 247, 251)");
    await expect(pageBody).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(darkThemeToggle).not.toBeChecked();
});

test("light theme keeps randomizer table text and disabled labels readable", async ({
    page,
}) => {
    await installThemeMock(page, {
        dark_theme: false,
        minimize_to_tray: false,
        start_minimized: true,
    });

    await page.goto("/", { waitUntil: "domcontentloaded" });

    const disabledLaunchRow = page
        .getByRole("checkbox", { name: "Start minimized" })
        .locator("xpath=ancestor::div[.//span][1]");
    const disabledLaunchLabel = disabledLaunchRow.getByText("Start minimized", {
        exact: true,
    });
    await expect(disabledLaunchLabel).toHaveCSS("color", "rgb(100, 116, 139)");
    await expect(disabledLaunchRow).toHaveCSS("opacity", "1");

    await page.getByRole("tab", { name: "Randomizer" }).click();

    const commanderCell = page
        .getByRole("row", { name: /Abathur/ })
        .first()
        .locator("td")
        .first();
    const commanderButton = page.getByRole("button", {
        name: "Toggle all prestiges for Abathur",
    });
    const headerButton = page.getByRole("button", {
        name: "Toggle P0 for all commanders",
    });

    await expect(commanderCell).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(commanderButton).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(commanderButton).toHaveCSS(
        "background-color",
        "rgba(0, 0, 0, 0)",
    );
    await expect(headerButton).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(headerButton).toHaveCSS(
        "background-color",
        "rgba(0, 0, 0, 0)",
    );
});

test("light theme keeps weekly detail chips readable", async ({ page }) => {
    await installThemeMock(page, {
        dark_theme: false,
    });

    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByRole("tab", { name: "Weeklies" }).click();

    const chip = page.getByText("Next In: Now", { exact: true });
    await expect(chip).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(chip).toHaveCSS("background-color", "rgb(237, 243, 250)");
});

test("light theme keeps player chips readable", async ({ page }) => {
    await installThemeMock(page, {
        dark_theme: false,
    });

    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByRole("tab", { name: "Players" }).click();
    const expander = page.getByRole("button", { name: "Expand" }).first();
    await expect(expander).toHaveCSS("background-color", "rgb(237, 243, 250)");
    await expect(expander).toHaveCSS("border-top-color", "rgb(215, 225, 238)");
    await expander.click();

    const chip = page.locator("code").filter({ hasText: "Main Tester" });
    await expect(chip).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(chip).toHaveCSS("background-color", "rgb(237, 243, 250)");
});

test("light theme keeps statistics empty states readable", async ({ page }) => {
    await installThemeMock(page, {
        dark_theme: false,
    });

    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByRole("tab", { name: "Statistics" }).click();

    const emptyState = page.getByText("No statistics loaded.", {
        exact: true,
    });
    await expect(emptyState).toHaveText("No statistics loaded.");
    await expect(emptyState).toHaveCSS("color", "rgb(51, 65, 85)");
    await expect(emptyState).toHaveCSS(
        "background-color",
        "rgb(237, 243, 250)",
    );
});

test("light theme keeps statistics filters and mastery graphs readable", async ({
    page,
}) => {
    await installThemeMock(
        page,
        {
            dark_theme: false,
        },
        LIGHT_THEME_READY_STATS,
    );

    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByRole("tab", { name: "Statistics" }).click();

    const difficultyHeading = page.getByRole("heading", {
        name: "Difficulty",
    });
    const difficultyPanel = difficultyHeading.locator("xpath=..");

    await expect(difficultyHeading).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(difficultyPanel).toHaveCSS(
        "background-color",
        "rgb(248, 251, 255)",
    );
    await expect(difficultyPanel).toHaveCSS(
        "border-top-color",
        "rgb(215, 225, 238)",
    );
    await expect(page.getByText("Casual", { exact: true })).toHaveCSS(
        "color",
        "rgb(15, 23, 42)",
    );

    await page.getByRole("button", { name: "My Commanders" }).click();

    const masteryHeader = page
        .getByTestId("mastery-distribution-header")
        .first()
        .locator("strong");
    const prestigeTitle = page
        .getByTestId("mastery-distribution-prestige-list")
        .first()
        .locator("h5")
        .first();
    const masteryGraph = page.getByTestId("mastery-distribution-line").first();
    const pointLabel = page
        .getByTestId("mastery-distribution-point-label")
        .first();

    await expect(masteryHeader).toHaveCSS("color", "rgb(29, 78, 216)");
    await expect(prestigeTitle).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(masteryGraph).toHaveCSS(
        "background-color",
        "rgb(238, 245, 253)",
    );
    await expect(pointLabel).toHaveCSS("background-color", "rgb(15, 23, 42)");
    await expect(pointLabel).toHaveCSS("color", "rgb(248, 250, 252)");
});
