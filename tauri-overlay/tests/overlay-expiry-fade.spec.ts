import { expect, test } from "@playwright/test";

test.describe.configure({ timeout: 60_000 });

test("auto-expired replay overlay uses the same fade-out before clearing replay data", async ({
    page,
}) => {
    await page.goto("/#/overlay", { waitUntil: "domcontentloaded" });
    await page.waitForFunction(
        () =>
            typeof (window as typeof window & { postGameStats?: unknown })
                .postGameStats === "function",
    );

    await page.evaluate(() => {
        const runtime = window as typeof window & {
            initColorsDuration: (data: {
                colors: [
                    string | null,
                    string | null,
                    string | null,
                    string | null,
                ];
                duration: number;
                show_charts: boolean;
            }) => void;
            postGameStats: (data: Record<string, unknown>) => void;
            showhide: () => void;
        };

        runtime.initColorsDuration({
            colors: [null, null, null, null],
            duration: 2,
            show_charts: false,
        });
        runtime.postGameStats({
            file: "fade-expiry-test.SC2Replay",
            mainPrestige: "Renegade Commander",
            allyPrestige: "Queen of Blades",
            comp: "Terran",
            player_stats: null,
            mutators: [],
            result: "Victory",
            mainCommander: "Raynor",
            allyCommander: "Kerrigan",
            bonus: [],
            map_name: "Chain of Ascension",
            length: 100,
            main: "Player One",
            ally: "Player Two",
            mainCommanderLevel: 15,
            allyCommanderLevel: 15,
            mainAPM: 100,
            allyAPM: 90,
            fastest: false,
            Victory: 1,
            Defeat: 0,
            difficulty: "Brutal",
            weekly: false,
            extension: 0,
            "B+": 0,
            mainkills: 10,
            allykills: 20,
            mainIcons: {},
            mainMasteries: [30, 0, 30, 0, 30, 0],
            mainUnits: {
                Marine: [5, 0, 10, 1],
            },
            allyIcons: {},
            allyMasteries: [0, 30, 0, 30, 0, 30],
            allyUnits: {
                Zergling: [8, 0, 20, 1],
            },
            amon_units: {},
            newReplay: true,
        });
    });

    await expect(page.locator("#stats")).toBeVisible();
    await expect(page.locator("#name1")).toHaveText("Player One");

    await page.waitForTimeout(1150);

    await expect(page.locator("#stats")).toBeVisible();
    await expect(page.locator("#stats")).toHaveCSS("opacity", "0");

    await page.waitForTimeout(1100);

    await expect(page.locator("#stats")).not.toBeVisible();
    await expect(page.locator("#name1")).toHaveText("");

    await page.evaluate(() => {
        const runtime = window as typeof window & {
            showhide: () => void;
        };
        runtime.showhide();
    });

    await expect(page.locator("#nodata")).toBeVisible();
    await expect(page.locator("#nodata")).toHaveText("NO DATA");
});
