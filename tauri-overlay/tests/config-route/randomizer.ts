import { test, expect } from "@playwright/test";
import { installTauriMock } from "../helpers/config-route-mock";

test.describe("Config route randomizer", () => {
    test.describe.configure({ timeout: 60000 });

    test("loads with hash route fallback", async ({ page }) => {
        await installTauriMock(page);
        await page.goto("/", { waitUntil: "domcontentloaded" });
        await expect(page).toHaveURL(/#\/config\/settings$/);
    });

    test("randomizer tab matches the legacy selection and result flow", async ({
        page,
    }) => {
        await installTauriMock(page);
        await page.goto("/", { waitUntil: "domcontentloaded" });

        await page.getByRole("tab", { name: "Randomizer" }).click();

        await expect(
            page.getByText("Commander and prestige choices"),
        ).toBeVisible();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P0" }),
        ).toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P1" }),
        ).not.toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Fenix P0" }),
        ).toBeChecked();
        await page
            .getByRole("button", { name: "Toggle all prestiges for Abathur" })
            .click();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P0" }),
        ).toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P1" }),
        ).toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P2" }),
        ).toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P3" }),
        ).toBeChecked();
        await page
            .getByRole("button", { name: "Toggle P0 for all commanders" })
            .click();
        await expect(
            page.getByRole("checkbox", { name: "Abathur P0" }),
        ).not.toBeChecked();
        await expect(
            page.getByRole("checkbox", { name: "Fenix P0" }),
        ).not.toBeChecked();
        await page.getByRole("button", { name: "Generate" }).first().click();

        await expect(
            page.getByText("Fenix - Purifier Executor (P0)"),
        ).toBeVisible();
        await expect(
            page.getByText("30 Champion A.I. Attack Speed"),
        ).toBeVisible();
        await expect(page.getByText("Scythe of Amon | Zerg")).toBeVisible();
    });
});
