import { expect, test } from "@playwright/test";
import { installConfigMock } from "./helpers/config-mock";

test.describe("Games filters and mutators", () => {
    test("filters include games beyond the first page", async ({ page }) => {
        await installConfigMock(page, {
            games: Array.from({ length: 305 }, (_, index) => ({
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
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Games" }).click();

        await page
            .getByRole("checkbox", { name: "Normal", exact: true })
            .click();

        await expect(page.locator("tbody tr").first()).toContainText("Map 305");
        await expect(
            page.getByText("Rows 1-1 of 1", { exact: true }).first(),
        ).toBeVisible();
    });

    test("loads later pages from paginated game queries", async ({ page }) => {
        await installConfigMock(page, {
            games: Array.from({ length: 305 }, (_, index) => ({
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
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Games" }).click();

        await expect(
            page.getByText("Rows 1-20 of 305", { exact: true }).first(),
        ).toBeVisible();

        for (let pageIndex = 1; pageIndex < 16; pageIndex += 1) {
            await page.getByRole("button", { name: "Next" }).last().click();
        }

        await expect(
            page.getByText("Rows 301-305 of 305", { exact: true }).first(),
        ).toBeVisible();
        await expect(page.locator("tbody tr").first()).toContainText("Map 301");
    });

    test("filters normal and mutation games and shows weekly difficulty notation", async ({
        page,
    }) => {
        await installConfigMock(page, {
            games: [
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
                            name: {
                                en: "Barrier",
                                ko: "방벽",
                            },
                            iconName: "Barrier",
                            description: {
                                en: "Enemy units gain a temporary shield when damaged.",
                                ko: "적 유닛이 피해를 받으면 일시적인 보호막을 얻습니다.",
                            },
                        },
                    ],
                },
            ],
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "Games" }).click();

        const rows = page.locator("tbody tr");
        await expect(rows).toHaveCount(2);
        await expect(rows.nth(0)).toContainText("Brutal (Weekly)");

        const mutatorIcon = page.getByRole("img", { name: "Barrier" });
        await expect(mutatorIcon).toHaveAttribute(
            "title",
            /Barrier\nEnemy units gain a temporary shield when damaged\./,
        );

        await page.getByRole("checkbox", { name: "Normal games" }).click();
        await expect(rows).toHaveCount(1);
        await expect(rows.nth(0)).toContainText("Malwarfare");
        await expect(
            page.getByRole("button", { name: "Previous" }).first(),
        ).toBeVisible();
        await expect(
            page.getByRole("button", { name: "Next" }).first(),
        ).toBeVisible();
        await expect(
            page.getByText("Rows 1-1 of 1", { exact: true }).first(),
        ).toBeVisible();

        await page.getByRole("checkbox", { name: "Mutations" }).click();
        await expect(
            page.getByRole("cell", { name: "No matching games" }),
        ).toHaveText("No matching games");
        await expect(
            page.getByRole("button", { name: "Previous" }).first(),
        ).toBeVisible();
        await expect(
            page.getByRole("button", { name: "Next" }).first(),
        ).toBeVisible();
        await expect(
            page.getByText("Rows 1-0 of 0", { exact: true }).first(),
        ).toBeVisible();

        await page.getByRole("checkbox", { name: "Mutations" }).click();
        await page
            .getByRole("checkbox", { name: "Brutal", exact: true })
            .click();
        await expect(
            page.getByRole("cell", { name: "No matching games" }),
        ).toHaveText("No matching games");
    });

    test("translates mixed difficulty labels from saved slash strings", async ({
        page,
    }) => {
        await installConfigMock(page, {
            settings: {
                language: "ko",
            },
            games: [
                {
                    map: "Void Launch",
                    result: "Victory",
                    p1: "Main",
                    p2: "Ally",
                    main_commander: "Abathur",
                    ally_commander: "Swann",
                    difficulty: "Hard/Brutal",
                    enemy: "Terran",
                    file: "mixed-difficulty.SC2Replay",
                    length: 900,
                    date: 1735689600,
                    weekly: false,
                    is_mutation: false,
                    mutators: [],
                },
            ],
        });

        await page.goto("/", { waitUntil: "domcontentloaded" });
        await page.getByRole("tab", { name: "게임" }).click();

        const rows = page.locator("tbody tr");
        await expect(rows).toHaveCount(1);
        await expect(rows.nth(0)).toContainText("어려움/아주 어려움");
    });
});
