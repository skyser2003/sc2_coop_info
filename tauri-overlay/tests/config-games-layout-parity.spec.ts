import { expect, test, type Page } from "@playwright/test";

type PlayerRow = {
    player: string;
    wins: number;
    losses: number;
    winrate: number;
    apm: number;
    commander: string;
    kills: number;
    last_seen: number;
};

type WeeklyRow = {
    mutation: string;
    difficulty: string;
    wins: number;
    losses: number;
    winrate: number;
};

type LayoutMockConfig = {
    players?: readonly PlayerRow[];
    weeklies?: readonly WeeklyRow[];
};

async function installLayoutMock(
    page: Page,
    { players = [], weeklies = [] }: LayoutMockConfig = {},
) {
    await page.addInitScript(
        ({ initialPlayers, initialWeeklies }) => {
            const settings = {
                account_folder: "fixtures/accounts",
                main_names: [],
                detailed_analysis_atstart: false,
                rng_choices: {},
            };
            let activeSettings = JSON.parse(JSON.stringify(settings));
            const cloneJson = <T>(value: T): T =>
                JSON.parse(JSON.stringify(value)) as T;
            const configPayload = () => ({
                status: "ok",
                settings,
                active_settings: activeSettings,
                randomizer_catalog: {},
                monitor_catalog: [],
            });
            const playersPayload = () => ({
                status: "ok",
                players: initialPlayers,
                total_players: initialPlayers.length,
                loading: false,
            });
            const weekliesPayload = () => ({
                status: "ok",
                weeklies: initialWeeklies,
            });
            const replaysPayload = () => ({
                status: "ok",
                replays: [],
                total_replays: 0,
                selected_replay_file: "",
            });

            window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener: () => {},
            };

            window.__TAURI_INTERNALS__ = {
                invoke: async (
                    command: string,
                    request?: {
                        body?: Record<string, unknown>;
                        method?: string;
                        path?: string;
                    },
                ) => {
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
                    if (command === "config_get") {
                        return configPayload();
                    }
                    if (command === "config_update") {
                        if (request?.body?.settings) {
                            activeSettings = cloneJson(
                                request.body.settings,
                            ) as typeof activeSettings;
                        }
                        return configPayload();
                    }
                    if (command === "config_players_get") {
                        return playersPayload();
                    }
                    if (command === "config_weeklies_get") {
                        return weekliesPayload();
                    }
                    if (command === "config_replays_get") {
                        return replaysPayload();
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

                    const method = request?.method;
                    const path = request?.path;

                    if (method === "GET" && path === "/config") {
                        return configPayload();
                    }

                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/players?")
                    ) {
                        return playersPayload();
                    }

                    if (method === "GET" && path === "/config/weeklies") {
                        return weekliesPayload();
                    }

                    if (
                        method === "GET" &&
                        path?.startsWith("/config/replays?")
                    ) {
                        return replaysPayload();
                    }

                    if (
                        method === "POST" &&
                        (path === "/config/action" ||
                            path === "/config/stats/action" ||
                            path === "/config")
                    ) {
                        if (path === "/config" && request?.body?.settings) {
                            activeSettings = JSON.parse(
                                JSON.stringify(request.body.settings),
                            );
                        }
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: {},
                            monitor_catalog: [],
                        };
                    }

                    throw new Error(
                        `Unexpected request: ${String(method)} ${String(path)}`,
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
        {
            initialPlayers: players,
            initialWeeklies: weeklies,
        },
    );
}

function buildPlayerRows(): PlayerRow[] {
    return Array.from({ length: 22 }, (_, index) => {
        const playerNumber = index + 1;
        return {
            player: `Player ${playerNumber}`,
            wins: playerNumber,
            losses: 22 - playerNumber,
            winrate: playerNumber / 22,
            apm: 100 + playerNumber,
            commander: "Fenix",
            kills: playerNumber / 20,
            last_seen: playerNumber,
        };
    });
}

function buildWeeklyRows(): WeeklyRow[] {
    return Array.from({ length: 22 }, (_, index) => {
        const weeklyNumber = index + 1;
        return {
            mutation: `Mutation ${weeklyNumber}`,
            difficulty: "Brutal",
            wins: weeklyNumber,
            losses: 22 - weeklyNumber,
            winrate: weeklyNumber / 22,
        };
    });
}

test.describe("Games layout parity", () => {
    test.describe.configure({ timeout: 60000 });

    test("players tab uses the games table shell", async ({ page }) => {
        await installLayoutMock(page, {
            players: buildPlayerRows(),
        });

        await page.goto("/#/config", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Players" }).click();

        const playersSection = page
            .getByRole("heading", { name: "Players" })
            .locator("xpath=ancestor::section[1]");
        const playersTable = playersSection.locator("table");

        await expect(playersSection).toBeVisible();
        await expect(
            playersSection.getByRole("textbox", { name: "Search" }),
        ).toBeVisible();
        await expect(playersTable).toBeVisible();
        await expect(playersTable.locator("tbody tr")).toHaveCount(20);
        await expect(
            playersSection
                .getByText("Rows 1-20 of 22", { exact: true })
                .first(),
        ).toBeVisible();
    });

    test("weeklies tab uses the games table shell and shows all rows", async ({
        page,
    }) => {
        await installLayoutMock(page, {
            weeklies: buildWeeklyRows(),
        });

        await page.goto("/#/config", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Weeklies" }).click();

        const weekliesSection = page
            .getByRole("heading", { name: "Weeklies" })
            .locator("xpath=ancestor::section[1]");
        const weekliesTable = weekliesSection.locator("table");
        const weekliesRows = weekliesTable.locator("tbody tr");

        await expect(weekliesSection).toBeVisible();
        await expect(weekliesSection.getByRole("button").first()).toBeVisible();
        await expect(weekliesTable).toBeVisible();
        await expect(weekliesRows).toHaveCount(22);
        await expect(weekliesRows.nth(0)).toContainText("Mutation 1");
        await expect(weekliesRows.nth(20)).toContainText("Mutation 21");
        await expect(weekliesRows.nth(21)).toContainText("Mutation 22");
        await expect(weekliesSection.getByText("Rows 1-20 of 22")).toHaveCount(
            0,
        );
        await expect(
            weekliesSection.getByRole("button", {
                name: "Next",
                exact: true,
            }),
        ).toHaveCount(0);
    });
});
