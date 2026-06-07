import { test, expect } from "@playwright/test";
import {
    expectedLocalReplayTimestamp,
    installTauriMock,
} from "../helpers/config-route-mock";

test.describe("Config route players", () => {
    test.describe.configure({ timeout: 60000 });

    test("players tab formats last seen as datetime", async ({ page }) => {
        await installTauriMock(page, null, [], {
            tabResponses: {
                players: {
                    status: "ok",
                    players: [
                        {
                            player: "AllyPlayer",
                            wins: 3,
                            losses: 1,
                            winrate: 0.75,
                            apm: 123,
                            commander: "Abathur",
                            kills: 0.41,
                            last_seen: 1538345544,
                        },
                    ],
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Players" }).click();

        const expectedTime = await expectedLocalReplayTimestamp(
            page,
            1538345544,
            true,
        );
        await expect(page.locator("tbody tr").nth(0)).toContainText(
            expectedTime,
        );
    });

    test("players tab defaults to last seen descending", async ({ page }) => {
        await installTauriMock(page, null, [], {
            tabResponses: {
                players: {
                    status: "ok",
                    players: [
                        {
                            player: "OlderPlayer",
                            wins: 1,
                            losses: 0,
                            winrate: 1,
                            apm: 80,
                            commander: "Karax",
                            kills: 0.2,
                            last_seen: 1538345544,
                        },
                        {
                            player: "NewerPlayer",
                            wins: 2,
                            losses: 1,
                            winrate: 0.66,
                            apm: 140,
                            commander: "Abathur",
                            kills: 0.5,
                            last_seen: 1735689600,
                        },
                    ],
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Players" }).click();

        await expect(page.locator("tbody tr").nth(0)).toContainText(
            "NewerPlayer",
        );
        await expect(page.locator("tbody tr").nth(1)).toContainText(
            "OlderPlayer",
        );
    });

    test("players tab can clear the active sort without crashing", async ({
        page,
    }) => {
        const pageErrors = [];
        page.on("pageerror", (error) => pageErrors.push(error.message));

        await installTauriMock(page, null, [], {
            tabResponses: {
                players: {
                    status: "ok",
                    players: [
                        {
                            handle: "3-S2-1-100",
                            player: "OlderPlayer",
                            player_names: ["OlderPlayer"],
                            wins: 1,
                            losses: 0,
                            winrate: 1,
                            apm: 80,
                            commander: "Karax",
                            frequency: 1,
                            kills: 0.2,
                            last_seen: 1538345544,
                        },
                        {
                            handle: "3-S2-1-200",
                            player: "NewerPlayer",
                            player_names: ["NewerPlayer"],
                            wins: 2,
                            losses: 1,
                            winrate: 0.66,
                            apm: 140,
                            commander: "Abathur",
                            frequency: 1,
                            kills: 0.5,
                            last_seen: 1735689600,
                        },
                    ],
                    total_players: 2,
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Players" }).click();
        await page.getByRole("button", { name: /^Last Seen/ }).click();

        await expect(
            page.getByRole("button", { name: "Last Seen" }),
        ).toBeVisible();
        await expect(page.locator("tbody tr").nth(0)).toContainText(
            "NewerPlayer",
        );
        expect(pageErrors).toEqual([]);
    });

    test("players tab requests the initial paginated player set", async ({
        page,
    }) => {
        await installTauriMock(page, null, [], {
            tabResponses: {
                players: {
                    status: "ok",
                    players: [],
                },
            },
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Players" }).click();

        await expect
            .poll(() =>
                page.evaluate(() => {
                    const requests = window["__SCO_TAB_REQUESTS__"] || [];
                    return (
                        requests.find(
                            (request) =>
                                request.command === "config_players_get",
                        ) || null
                    );
                }),
            )
            .toMatchObject({
                command: "config_players_get",
                request: {
                    request: {
                        page: 1,
                        rowsPerPage: 20,
                        search: "",
                        sortKey: "last_seen",
                        sortDirection: "desc",
                    },
                },
            });
    });
});
