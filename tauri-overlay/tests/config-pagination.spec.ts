import { expect, test } from "@playwright/test";
import {
    installConfigMock,
    type ConfigMockGameRow,
    type ConfigMockPlayerRow,
} from "./helpers/config-mock";

function buildGameRows(): ConfigMockGameRow[] {
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

function buildPlayerRows(): ConfigMockPlayerRow[] {
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
        await installConfigMock(page, {
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
        await installConfigMock(page, {
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
        await installConfigMock(page, {
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
        await expect(page.locator("tbody tr").first()).toContainText(
            "Player 301",
        );
    });
});
