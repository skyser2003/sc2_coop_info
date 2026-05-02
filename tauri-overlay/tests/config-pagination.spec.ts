import { expect, test, type Page } from "@playwright/test";

type GameRow = {
    map: string;
    result: string;
    p1: string;
    p2: string;
    main_commander: string;
    ally_commander: string;
    difficulty: string;
    enemy_race: string;
    file: string;
    length: number;
    date: number;
};

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

type TauriMockConfig = {
    games?: readonly GameRow[];
    players?: readonly PlayerRow[];
};

async function installPaginationMock(
    page: Page,
    { games = [], players = [] }: TauriMockConfig = {},
) {
    await page.addInitScript(
        ({ initialGames, initialPlayers }) => {
            const settings = {
                account_folder: "fixtures/accounts",
                main_names: [],
                detailed_analysis_atstart: false,
                rng_choices: {},
            };
            const activeSettings = JSON.parse(JSON.stringify(settings));

            window.__TAURI_INTERNALS__ = {
                invoke: async (
                    command: string,
                    request?: {
                        body?: Record<string, unknown>;
                        limit?: number;
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
                        return {
                            status: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: {},
                            monitor_catalog: [],
                        };
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

                    if (command === "config_stats_action") {
                        return { status: "ok", message: "ok" };
                    }

                    if (command === "config_replays_get") {
                        const limit = Number(request?.limit || 300);
                        return {
                            status: "ok",
                            replays: initialGames.slice(
                                0,
                                Number.isFinite(limit) && limit > 0
                                    ? limit
                                    : initialGames.length,
                            ),
                            total_replays: initialGames.length,
                            selected_replay_file: "",
                        };
                    }

                    if (command === "config_players_get") {
                        const limit = Number(request?.limit || 300);
                        return {
                            status: "ok",
                            players: initialPlayers.slice(
                                0,
                                Number.isFinite(limit) && limit > 0
                                    ? limit
                                    : initialPlayers.length,
                            ),
                            total_players: initialPlayers.length,
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

                    if (method === "POST" && path === "/config/stats/action") {
                        return { status: "ok", message: "ok" };
                    }

                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/replays?")
                    ) {
                        const url = new URL(path, "http://127.0.0.1");
                        const limit = Number(
                            url.searchParams.get("limit") || "300",
                        );
                        return {
                            status: "ok",
                            replays: initialGames.slice(
                                0,
                                Number.isFinite(limit) && limit > 0
                                    ? limit
                                    : initialGames.length,
                            ),
                            total_replays: initialGames.length,
                            selected_replay_file: "",
                        };
                    }

                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/players?")
                    ) {
                        const url = new URL(path, "http://127.0.0.1");
                        const limit = Number(
                            url.searchParams.get("limit") || "300",
                        );
                        return {
                            status: "ok",
                            players: initialPlayers.slice(
                                0,
                                Number.isFinite(limit) && limit > 0
                                    ? limit
                                    : initialPlayers.length,
                            ),
                            total_players: initialPlayers.length,
                            loading: false,
                        };
                    }

                    if (method === "POST" && path === "/config/replays/show") {
                        return { status: "ok", message: "Replay sent" };
                    }

                    if (method === "POST" && path === "/config/stats/action") {
                        return { status: "ok", message: "ok" };
                    }

                    if (method === "POST" && path === "/config/action") {
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
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
            window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener: () => {},
            };
        },
        {
            initialGames: games,
            initialPlayers: players,
        },
    );
}

function buildGameRows(): GameRow[] {
    return Array.from({ length: 22 }, (_, index) => {
        const gameNumber = index + 1;
        return {
            map: `Map ${gameNumber}`,
            result: gameNumber % 2 === 0 ? "Victory" : "Defeat",
            p1: `Player ${gameNumber}`,
            p2: `Ally ${gameNumber}`,
            main_commander: "Abathur",
            ally_commander: "Karax",
            difficulty: "Brutal",
            enemy_race: "Zerg",
            file: `fixtures/replays/replay-${gameNumber}.SC2Replay`,
            length: 1000 + gameNumber,
            date: gameNumber,
        };
    });
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

test.describe("Config pagination", () => {
    test.describe.configure({ timeout: 60000 });

    test("games tab paginates after applying the default time sort", async ({
        page,
    }) => {
        await installPaginationMock(page, {
            games: buildGameRows(),
        });

        await page.goto("/#/config", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Games" }).click();

        const gamesRows = page.locator("tbody tr");
        await expect(gamesRows).toHaveCount(20);
        await expect(
            page.getByText("Rows 1-20 of 22", { exact: true }).first(),
        ).toBeVisible();
        await expect(gamesRows.nth(0)).toContainText("Player 22");
        await expect(gamesRows.nth(19)).toContainText("Player 3");

        await page.getByRole("button", { name: "Next" }).last().click();

        await expect(gamesRows).toHaveCount(2);
        await expect(
            page.getByText("Rows 21-22 of 22", { exact: true }).first(),
        ).toBeVisible();
        await expect(gamesRows.nth(0)).toContainText("Player 2");
        await expect(gamesRows.nth(1)).toContainText("Player 1");
    });

    test("players tab paginates after applying the default last seen sort", async ({
        page,
    }) => {
        await installPaginationMock(page, {
            players: buildPlayerRows(),
        });

        await page.goto("/#/config", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Players" }).click();

        const playerRows = page.locator("tbody tr");
        await expect(playerRows).toHaveCount(20);
        await expect(
            page.getByText("Rows 1-20 of 22", { exact: true }).first(),
        ).toBeVisible();
        await expect(playerRows.nth(0)).toContainText("Player 22");
        await expect(playerRows.nth(19)).toContainText("Player 3");

        await page.getByRole("button", { name: "Next" }).last().click();

        await expect(playerRows).toHaveCount(2);
        await expect(
            page.getByText("Rows 21-22 of 22", { exact: true }).first(),
        ).toBeVisible();
        await expect(playerRows.nth(0)).toContainText("Player 2");
        await expect(playerRows.nth(1)).toContainText("Player 1");
    });

    test("players tab loads beyond the initial 300 rows when paging forward", async ({
        page,
    }) => {
        await installPaginationMock(page, {
            players: Array.from({ length: 305 }, (_, index) => {
                const playerNumber = index + 1;
                return {
                    player: `Player ${playerNumber}`,
                    wins: playerNumber,
                    losses: 0,
                    winrate: 1,
                    apm: 100,
                    commander: "Fenix",
                    kills: 0.5,
                    last_seen: 305 - index,
                };
            }),
        });

        await page.goto("/#/config", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Players" }).click();

        await expect(
            page.getByText("Rows 1-20 of 305", { exact: true }).first(),
        ).toBeVisible();

        for (let pageIndex = 1; pageIndex < 16; pageIndex += 1) {
            await page.getByRole("button", { name: "Next" }).last().click();
        }

        await expect(
            page.getByText("Rows 301-305 of 305", { exact: true }).first(),
        ).toBeVisible();
        await expect(
            page.locator("tbody tr").first(),
        ).toContainText("Player 301");
    });
});
