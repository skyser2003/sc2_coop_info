import { expect, test, type Page } from "@playwright/test";

type GamesMutator = {
    name: string;
    nameEn: string;
    nameKo: string;
    iconName: string;
    descriptionEn: string;
    descriptionKo: string;
};

type GamesRow = {
    map: string;
    result: string;
    p1: string;
    p2: string;
    main_commander: string;
    ally_commander: string;
    difficulty: string;
    enemy: string;
    file: string;
    length: number;
    date: number;
    brutal_plus?: number;
    weekly?: boolean;
    is_mutation?: boolean;
    mutators?: readonly GamesMutator[];
};

async function installGamesMock(page: Page, rows: readonly GamesRow[]) {
    await page.addInitScript(
        ({ initialRows }) => {
            const settings = {
                account_folder: "fixtures/accounts",
                main_names: [],
                detailed_analysis_atstart: false,
                rng_choices: {},
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
                    if (command !== "config_request") {
                        throw new Error(`Unexpected command: ${command}`);
                    }

                    const method = request?.method;
                    const path = request?.path;

                    if (method === "GET" && path === "/config") {
                        return {
                            status: "ok",
                            settings,
                            active_settings: settings,
                            randomizer_catalog: {},
                            monitor_catalog: [],
                        };
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
                            replays: initialRows.slice(
                                0,
                                Number.isFinite(limit) && limit > 0
                                    ? limit
                                    : initialRows.length,
                            ),
                            total_replays: initialRows.length,
                            selected_replay_file: "",
                        };
                    }

                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/players?")
                    ) {
                        return {
                            status: "ok",
                            players: [],
                        };
                    }

                    if (method === "GET" && path === "/config/weeklies") {
                        return {
                            status: "ok",
                            weeklies: [],
                        };
                    }

                    if (
                        method === "POST" &&
                        (path === "/config" ||
                            path === "/config/action" ||
                            path === "/config/stats/action")
                    ) {
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
                            settings,
                            active_settings: settings,
                            randomizer_catalog: {},
                            monitor_catalog: [],
                        };
                    }

                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/stats?")
                    ) {
                        return {
                            status: "ok",
                            stats: {
                                ready: true,
                                games: 0,
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

                    throw new Error(
                        `Unexpected request: ${String(method)} ${String(path)}`,
                    );
                },
                event: {
                    listen: async () => () => {},
                },
            };
        },
        { initialRows: rows },
    );
}

test.describe("Games filters and mutators", () => {
    test("filters include older games beyond the initially loaded 300 rows", async ({
        page,
    }) => {
        await installGamesMock(
            page,
            Array.from({ length: 305 }, (_, index) => ({
                map: `Map ${index + 1}`,
                result: "Victory",
                p1: `Player ${index + 1}`,
                p2: "Ally",
                main_commander: "Abathur",
                ally_commander: "Swann",
                difficulty: index === 304 ? "Brutal" : "Normal",
                enemy: "Terran",
                file: `game-${index + 1}.SC2Replay`,
                length: 900,
                date: 1735689600 - index,
                weekly: false,
                is_mutation: false,
                mutators: [],
            })),
        );

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Games" }).click();

        await page
            .getByRole("checkbox", { name: "Normal", exact: true })
            .click();

        await expect(
            page.locator("table.games-table tbody tr").first(),
        ).toContainText("Map 305");
        await expect(
            page.getByText("Rows 1-1 of 1", { exact: true }),
        ).toBeVisible();
    });

    test("loads beyond the initial 300 rows when paging forward", async ({
        page,
    }) => {
        await installGamesMock(
            page,
            Array.from({ length: 305 }, (_, index) => ({
                map: `Map ${index + 1}`,
                result: "Victory",
                p1: `Player ${index + 1}`,
                p2: "Ally",
                main_commander: "Abathur",
                ally_commander: "Swann",
                difficulty: "Normal",
                enemy: "Terran",
                file: `game-${index + 1}.SC2Replay`,
                length: 900,
                date: 1735689600 - index,
                weekly: false,
                is_mutation: false,
                mutators: [],
            })),
        );

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Games" }).click();

        await expect(
            page.getByText("Rows 1-20 of 305", { exact: true }),
        ).toBeVisible();

        for (let pageIndex = 1; pageIndex < 16; pageIndex += 1) {
            await page.getByRole("button", { name: "Next" }).click();
        }

        await expect(
            page.getByText("Rows 301-305 of 305", { exact: true }),
        ).toBeVisible();
        await expect(
            page.locator("table.games-table tbody tr").first(),
        ).toContainText("Map 301");
    });

    test("filters normal and mutation games and shows weekly difficulty notation", async ({
        page,
    }) => {
        await installGamesMock(page, [
            {
                map: "Void Launch",
                result: "Victory",
                p1: "Main",
                p2: "Ally",
                main_commander: "Abathur",
                ally_commander: "Swann",
                difficulty: "Normal",
                enemy: "Terran",
                file: "normal.SC2Replay",
                length: 900,
                date: 1735689600,
                weekly: false,
                is_mutation: false,
                mutators: [],
            },
            {
                map: "Malwarfare",
                result: "Victory",
                p1: "Main",
                p2: "Ally",
                main_commander: "Abathur",
                ally_commander: "Stukov",
                difficulty: "Brutal",
                enemy: "Zerg",
                file: "weekly.SC2Replay",
                length: 1200,
                date: 1735776000,
                weekly: true,
                is_mutation: true,
                mutators: [
                    {
                        name: "Barrier",
                        nameEn: "Barrier",
                        nameKo: "방벽",
                        iconName: "Barrier",
                        descriptionEn:
                            "Enemy units gain a temporary shield when damaged.",
                        descriptionKo:
                            "적 유닛이 피해를 받으면 일시적인 보호막을 얻습니다.",
                    },
                ],
            },
        ]);

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("button", { name: "Games" }).click();

        const rows = page.locator("table.games-table tbody tr");
        await expect(rows).toHaveCount(2);
        await expect(rows.nth(0)).toContainText("Brutal (Weekly)");

        const mutatorIcon = page.locator(".games-mutator-icon").first();
        await expect(mutatorIcon).toHaveAttribute(
            "title",
            /Barrier\nEnemy units gain a temporary shield when damaged\./,
        );

        await page.getByRole("checkbox", { name: "Normal games" }).click();
        await expect(rows).toHaveCount(1);
        await expect(rows.nth(0)).toContainText("Malwarfare");
        await expect(
            page.getByRole("button", { name: "Previous" }),
        ).toBeVisible();
        await expect(page.getByRole("button", { name: "Next" })).toBeVisible();
        await expect(
            page.getByText("Rows 1-1 of 1", { exact: true }),
        ).toBeVisible();

        await page.getByRole("checkbox", { name: "Mutations" }).click();
        await expect(
            page.locator("table.games-table tbody .empty-cell"),
        ).toHaveText("No matching games");
        await expect(
            page.getByRole("button", { name: "Previous" }),
        ).toBeVisible();
        await expect(page.getByRole("button", { name: "Next" })).toBeVisible();
        await expect(
            page.getByText("Rows 1-0 of 0", { exact: true }),
        ).toBeVisible();

        await page.getByRole("checkbox", { name: "Mutations" }).click();
        await page
            .getByRole("checkbox", { name: "Brutal", exact: true })
            .click();
        await expect(
            page.locator("table.games-table tbody .empty-cell"),
        ).toHaveText("No matching games");
    });
});
