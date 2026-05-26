import { expect, test, type Page } from "@playwright/test";
import { installConfigMock } from "./helpers/config-mock";

type ConfigTabCommand = "config_replays_get" | "config_players_get";

async function tabRequestCount(
    page: Page,
    command: ConfigTabCommand,
): Promise<number> {
    return page.evaluate((commandName) => {
        return window.__SCO_TAB_REQUESTS__.filter(
            (request) => request?.command === commandName,
        ).length;
    }, command);
}

async function configGetRequestCount(page: Page): Promise<number> {
    return page.evaluate(() => window.__SCO_CONFIG_GET_REQUESTS__.length);
}

async function statsRequestCount(page: Page): Promise<number> {
    return page.evaluate(() => window.__SCO_STATS_REQUESTS__.length);
}

async function expectStableSettingsStartupConfigLoads(
    page: Page,
): Promise<void> {
    await expect.poll(() => configGetRequestCount(page)).toBeGreaterThan(0);
    await page.waitForTimeout(150);
    const requestCount = await configGetRequestCount(page);
    expect(requestCount).toBeGreaterThanOrEqual(1);
    expect(requestCount).toBeLessThanOrEqual(2);
}

async function expectStableTabRequestCount(
    page: Page,
    command: ConfigTabCommand,
    expectedCount: number,
): Promise<void> {
    await expect.poll(() => tabRequestCount(page, command)).toBe(expectedCount);
    await page.waitForTimeout(150);
    expect(await tabRequestCount(page, command)).toBe(expectedCount);
}

test.describe("Config tab data loading", () => {
    test("does not load full statistics on settings startup", async ({
        page,
    }) => {
        await installConfigMock(page);

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });

        await expectStableSettingsStartupConfigLoads(page);
        expect(await statsRequestCount(page)).toBe(0);

        await page.getByRole("tab", { name: "Statistics" }).click();
        await expect.poll(() => statsRequestCount(page)).toBe(1);
    });

    test("loads a data tab once when switching tabs", async ({ page }) => {
        await installConfigMock(page);

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });
        await page.getByRole("tab", { name: "Games" }).click();
        await expectStableTabRequestCount(page, "config_replays_get", 1);

        await page.getByRole("tab", { name: "Players" }).click();
        await expectStableTabRequestCount(page, "config_players_get", 1);
        expect(await tabRequestCount(page, "config_replays_get")).toBe(1);
    });
});
