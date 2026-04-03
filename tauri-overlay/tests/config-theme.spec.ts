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

async function installThemeMock(
    page: Page,
    settingsOverride: ConfigSettings = {},
) {
    await page.addInitScript(
        ({ providedSettings }) => {
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
            };

            (
                window as typeof window & {
                    __TAURI_INTERNALS__?: {
                        invoke: (
                            command: string,
                            request: {
                                path?: string;
                                method?: string;
                                body?: {
                                    settings?: typeof settings;
                                    persist?: boolean;
                                };
                            },
                        ) => Promise<unknown>;
                        event: {
                            listen: () => Promise<() => void>;
                        };
                    };
                }
            ).__TAURI_INTERNALS__ = {
                invoke: async (command, request) => {
                    if (command !== "config_request") {
                        throw new Error(`Unexpected command: ${command}`);
                    }

                    if (
                        request.method === "GET" &&
                        request.path === "/config"
                    ) {
                        return {
                            status: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: randomizerCatalog,
                            monitor_catalog: [
                                { index: 1, label: "1 - Primary Monitor" },
                            ],
                        };
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

                        return {
                            status: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: randomizerCatalog,
                            monitor_catalog: [
                                { index: 1, label: "1 - Primary Monitor" },
                            ],
                        };
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
                        return {
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
                        };
                    }

                    if (
                        request.method === "GET" &&
                        request.path.startsWith("/config/players?")
                    ) {
                        return {
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
                        };
                    }

                    if (
                        request.method === "GET" &&
                        request.path.startsWith("/config/stats?")
                    ) {
                        return {
                            status: "ok",
                            stats: {
                                ready: false,
                                games: 0,
                                analysis_running: false,
                                analysis_running_mode: null,
                                message: "",
                            },
                        };
                    }

                    throw new Error(
                        `Unexpected request: ${request.method} ${request.path}`,
                    );
                },
                event: {
                    listen: async () => () => {},
                },
            };
        },
        { providedSettings: settingsOverride },
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

    const disabledLaunchRow = page.locator(".main-setting-check.is-disabled");
    const disabledLaunchLabel = disabledLaunchRow.locator("span");
    await expect(disabledLaunchLabel).toHaveCSS("color", "rgb(100, 116, 139)");
    await expect(disabledLaunchRow).toHaveCSS("opacity", "1");

    await page.getByRole("button", { name: "Randomizer" }).click();

    const commanderCell = page
        .locator(".randomizer-choice-table tbody tr")
        .first()
        .locator("td")
        .first();
    const commanderButton = commanderCell.locator(
        ".randomizer-commander-toggle",
    );
    const headerButton = page.locator(".randomizer-header-toggle").first();

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
    await page.getByRole("button", { name: "Weeklies" }).click();

    const chip = page.locator(".weeklies-stat-chip").first();
    await expect(chip).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(chip).toHaveCSS("background-color", "rgb(237, 243, 250)");
});

test("light theme keeps player chips readable", async ({ page }) => {
    await installThemeMock(page, {
        dark_theme: false,
    });

    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByRole("button", { name: "Players" }).click();
    const expander = page.locator(".players-expander-btn").first();
    await expect(expander).toHaveCSS("background-color", "rgb(237, 243, 250)");
    await expect(expander).toHaveCSS("border-top-color", "rgb(215, 225, 238)");
    await expander.click();

    const chip = page.locator(".players-handle-chip").first();
    await expect(chip).toHaveCSS("color", "rgb(15, 23, 42)");
    await expect(chip).toHaveCSS("background-color", "rgb(237, 243, 250)");
});

test("light theme keeps statistics empty states readable", async ({ page }) => {
    await installThemeMock(page, {
        dark_theme: false,
    });

    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.getByRole("button", { name: "Statistics" }).click();

    const emptyState = page.locator(".stats-detail-empty").first();
    await expect(emptyState).toHaveText("No statistics loaded.");
    await expect(emptyState).toHaveCSS("color", "rgb(51, 65, 85)");
    await expect(emptyState).toHaveCSS(
        "background-color",
        "rgb(237, 243, 250)",
    );
});
