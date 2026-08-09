import { expect, test } from "@playwright/test";
import {
    installConfigMock,
    type ConfigMockGitHubRelease,
} from "./helpers/config-mock";

const GITHUB_RELEASES_PAGE_URL =
    "https://github.com/skyser2003/sc2_coop_info/releases";

function mockReleases(): readonly ConfigMockGitHubRelease[] {
    return Array.from({ length: 12 }, (_, index) => {
        const version = 12 - index;
        return {
            tag_name: `v2.${version}`,
            body: `Feature ${version}\nBug fix ${version}\nMaintenance ${version}`,
            draft: false,
            html_url: `${GITHUB_RELEASES_PAGE_URL}/tag/v2.${version}`,
        };
    });
}

test.describe("Settings patch notes", () => {
    test("shows the latest ten releases in a scrollable panel", async ({
        page,
    }) => {
        await installConfigMock(page, {
            githubReleases: mockReleases(),
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });

        const panel = page.getByRole("region", { name: "Patch notes" });
        const patchNotes = panel.getByTestId("patch-note");
        await expect(panel).toBeVisible();
        await expect(
            panel.getByRole("link", { name: "Patch notes" }),
        ).toHaveAttribute("href", GITHUB_RELEASES_PAGE_URL);
        await expect(patchNotes).toHaveCount(10);
        await expect(
            panel.getByRole("heading", { name: "v2.12" }),
        ).toBeVisible();
        await expect(panel.getByText("Feature 12")).toBeVisible();
        await expect(
            panel.getByRole("heading", { name: "v2.3" }),
        ).toBeVisible();
        await expect(panel.getByRole("heading", { name: "v2.2" })).toHaveCount(
            0,
        );

        const scrollArea = panel.getByTestId("patch-notes-scroll");
        await expect
            .poll(() =>
                scrollArea.evaluate(
                    (element) => element.scrollHeight > element.clientHeight,
                ),
            )
            .toBe(true);
        await expect(
            scrollArea.getByRole("button", { name: "Show more" }),
        ).toBeVisible();
    });

    test("opens the linked releases overview and version pages", async ({
        page,
    }) => {
        await installConfigMock(page, {
            githubReleases: mockReleases(),
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });

        const panel = page.getByRole("region", { name: "Patch notes" });
        await expect(
            panel.getByRole("button", { name: "Open releases on GitHub" }),
        ).toHaveCount(0);
        await panel.getByRole("link", { name: "Patch notes" }).click();
        await panel.getByRole("link", { name: "v2.12" }).click();
        await panel.getByRole("button", { name: "Show more" }).click();

        await expect
            .poll(() =>
                page.evaluate(() => [...window.__SCO_OPEN_URL_REQUESTS__]),
            )
            .toEqual([
                GITHUB_RELEASES_PAGE_URL,
                `${GITHUB_RELEASES_PAGE_URL}/tag/v2.12`,
                GITHUB_RELEASES_PAGE_URL,
            ]);
    });

    test("shows now before latest when the current version is newest", async ({
        page,
    }) => {
        const releases = [...mockReleases()];
        releases[0] = {
            tag_name: "v0.1.0",
            body: "Current release",
            draft: false,
            html_url: `${GITHUB_RELEASES_PAGE_URL}/tag/v0.1.0`,
        };
        await installConfigMock(page, {
            githubReleases: releases,
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });

        const firstPatchNote = page.getByTestId("patch-note").first();
        await expect(firstPatchNote.getByTestId("patch-note-label")).toHaveText(
            ["now", "latest"],
        );
    });

    test("reloads releases on every Settings tab click", async ({ page }) => {
        let releaseRequestCount = 0;
        await installConfigMock(page, {
            githubReleases: mockReleases(),
            onGitHubReleaseRequest: () => {
                releaseRequestCount += 1;
            },
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });
        await expect(page.getByTestId("patch-note")).toHaveCount(10);
        const initialRequestCount = releaseRequestCount;
        const settingsTab = page.getByRole("tab", { name: "Settings" });

        await settingsTab.click();
        await expect
            .poll(() => releaseRequestCount)
            .toBe(initialRequestCount + 1);

        await settingsTab.click();
        await expect
            .poll(() => releaseRequestCount)
            .toBe(initialRequestCount + 2);
    });
});
