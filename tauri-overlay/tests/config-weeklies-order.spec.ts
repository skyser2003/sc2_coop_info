import { expect, test, type Page } from "@playwright/test";

type WeeklyRow = {
    mutation: string;
    mutationOrder: number;
    isCurrent?: boolean;
    nextDuration?: string;
    nextDurationDays?: number;
    difficulty: string;
    wins: number;
    losses: number;
    winrate: number;
};

async function installWeekliesMock(page: Page, weeklies: readonly WeeklyRow[]) {
    await page.addInitScript(
        ({ initialWeeklies }) => {
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

                    if (method === "GET" && path === "/config/weeklies") {
                        return {
                            status: "ok",
                            weeklies: initialWeeklies,
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
            initialWeeklies: weeklies,
        },
    );
}

test("weeklies default to next duration and mutation sort is alphabetical", async ({
    page,
}) => {
    await installWeekliesMock(page, [
        {
            mutation: "Time Lock",
            mutationOrder: 2,
            isCurrent: false,
            nextDuration: "2w",
            nextDurationDays: 14,
            difficulty: "Brutal",
            wins: 3,
            losses: 1,
            winrate: 0.75,
        },
        {
            mutation: "Train of the Dead",
            mutationOrder: 0,
            isCurrent: true,
            nextDuration: "Now",
            nextDurationDays: 0,
            difficulty: "Brutal",
            wins: 5,
            losses: 0,
            winrate: 1,
        },
        {
            mutation: "First Strike",
            mutationOrder: 1,
            isCurrent: false,
            nextDuration: "1w",
            nextDurationDays: 7,
            difficulty: "Brutal",
            wins: 4,
            losses: 2,
            winrate: 0.667,
        },
    ]);

    await page.goto("/#/config", { waitUntil: "domcontentloaded" });
    await page.getByRole("button", { name: "Weeklies" }).click();

    const rows = page.locator("section.games-panel table.games-table tbody tr");
    await expect(rows).toHaveCount(3);
    await expect(rows.nth(0)).toContainText("Train of the Dead");
    await expect(rows.nth(0)).toContainText("Now");
    await expect(rows.nth(1)).toContainText("First Strike");
    await expect(rows.nth(2)).toContainText("Time Lock");

    await page.getByRole("button", { name: "Mutation" }).click();
    await expect(rows.nth(0)).toContainText("First Strike");
    await expect(rows.nth(1)).toContainText("Time Lock");
    await expect(rows.nth(2)).toContainText("Train of the Dead");

    await page.getByRole("button", { name: "Mutation ▲" }).click();
    await expect(rows.nth(0)).toContainText("Train of the Dead");
    await expect(rows.nth(1)).toContainText("Time Lock");
    await expect(rows.nth(2)).toContainText("First Strike");

    await page.getByRole("button", { name: "Next In" }).click();
    await expect(rows.nth(0)).toContainText("Train of the Dead");
    await expect(rows.nth(1)).toContainText("First Strike");
    await expect(rows.nth(2)).toContainText("Time Lock");

    await page.getByRole("button", { name: "Next In ▲" }).click();
    await expect(rows.nth(0)).toContainText("Time Lock");
    await expect(rows.nth(1)).toContainText("First Strike");
    await expect(rows.nth(2)).toContainText("Train of the Dead");
});
