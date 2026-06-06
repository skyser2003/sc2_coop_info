import { expect, test, type Page } from "@playwright/test";
import { installConfigMock } from "./helpers/config-mock";

class AnalyticsRequestTracker {
    private readonly requestedUrls: string[] = [];

    public constructor(page: Page) {
        page.on("request", (request) => {
            const urlText = request.url();
            if (AnalyticsRequestTracker.isAnalyticsUrl(urlText)) {
                this.requestedUrls.push(urlText);
            }
        });
    }

    public urls(): readonly string[] {
        return this.requestedUrls;
    }

    private static isAnalyticsUrl(urlText: string): boolean {
        const url = new URL(urlText);
        return (
            url.hostname === "www.googletagmanager.com" ||
            url.hostname.endsWith(".google-analytics.com") ||
            url.hostname.endsWith(".analytics.google.com")
        );
    }
}

test("does not serve Google Analytics from the dev server", async ({
    page,
}) => {
    const analyticsRequests = new AnalyticsRequestTracker(page);

    await installConfigMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });
    await page.waitForTimeout(500);

    await expect(
        page.locator('script[src*="googletagmanager.com/gtag/js"]'),
    ).toHaveCount(0);
    await expect(page.locator('script[nonce="c2NvT3ZlcmxheUdh"]')).toHaveCount(
        0,
    );
    expect(analyticsRequests.urls()).toEqual([]);
});
