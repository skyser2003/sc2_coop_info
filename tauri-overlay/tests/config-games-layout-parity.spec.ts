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

            window.__TAURI_INTERNALS__ = {
                invoke: async (
                    command: string,
                    request?: {
                        body?: Record<string, unknown>;
                        method?: string;
                        path?: string;
                    },
                ) => {
                    if (command !== "config_request") {
                        throw new Error(`Unexpected command: ${command}`);
                    }

                    const method = request?.method;
                    const path = request?.path;

                    if (method === "GET" && path === "/config") {
                        return {
                            status: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: {},
                            monitor_catalog: [],
                        };
                    }

                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/players?")
                    ) {
                        return {
                            status: "ok",
                            players: initialPlayers,
                        };
                    }

                    if (method === "GET" && path === "/config/weeklies") {
                        return {
                            status: "ok",
                            weeklies: initialWeeklies,
                        };
                    }

                    if (
                        method === "GET" &&
                        path?.startsWith("/config/replays?")
                    ) {
                        return {
                            status: "ok",
                            replays: [],
                            selected_replay_file: "",
                        };
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
        await page.getByRole("button", { name: "Players" }).click();

        const playersSection = page.locator("section.games-panel");
        const playersTable = playersSection.locator("table.games-table");

        await expect(playersSection).toBeVisible();
        await expect(playersSection.locator(".games-toolbar")).toBeVisible();
        await expect(playersTable).toBeVisible();
        await expect(playersTable.locator("tbody tr")).toHaveCount(20);
        await expect(
            playersSection.getByText("Rows 1-20 of 22", { exact: true }),
        ).toBeVisible();
    });

    test("weeklies tab uses the games table shell and shows all rows", async ({
        page,
    }) => {
        await installLayoutMock(page, {
            weeklies: buildWeeklyRows(),
        });

        await page.goto("/#/config", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Weeklies" }).click();

        const weekliesSection = page.locator("section.games-panel");
        const weekliesTable = weekliesSection.locator("table.games-table");
        const weekliesRows = weekliesTable.locator("tbody tr");

        await expect(weekliesSection).toBeVisible();
        await expect(weekliesSection.locator(".games-toolbar")).toBeVisible();
        await expect(weekliesTable).toBeVisible();
        await expect(weekliesRows).toHaveCount(22);
        await expect(weekliesRows.nth(0)).toContainText("Mutation 1");
        await expect(weekliesRows.nth(20)).toContainText("Mutation 21");
        await expect(weekliesRows.nth(21)).toContainText("Mutation 22");
        await expect(weekliesSection.getByText("Rows 1-20 of 22")).toHaveCount(
            0,
        );
        await expect(
            weekliesSection.getByRole("button", { name: "Next" }),
        ).toHaveCount(0);
    });
});
