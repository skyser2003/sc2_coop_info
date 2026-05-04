import { expect, test } from "@playwright/test";

test.describe.configure({ timeout: 60_000 });

test("expired overlay replay data clears and falls back to NO DATA on manual toggle", async ({
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
        };

        runtime.initColorsDuration({
            colors: [null, null, null, null],
            duration: 2,
            show_charts: false,
        });
        runtime.postGameStats({
            file: "test.SC2Replay",
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
            mainMasteryLevel: 90,
            allyMasteryLevel: 90,
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

    await expect(page.locator("#name1")).toHaveText("Player One");

    await page.waitForTimeout(2250);

    await expect(page.locator("#name1")).toHaveText("");

    await page.evaluate(() => {
        const runtime = window as typeof window & {
            showhide: () => void;
        };
        runtime.showhide();
    });

    await expect(page.locator("#nodata")).toBeVisible();
    await expect(page.locator("#nodata")).toHaveText("NO DATA");
    await expect(page.locator("#name1")).toHaveText("");
    await expect(page.locator("#map")).toHaveText("");
});
