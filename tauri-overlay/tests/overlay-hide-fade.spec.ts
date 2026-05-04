import { expect, test } from "@playwright/test";

test.describe.configure({ timeout: 60_000 });

test("manual replay overlay hide fades the full stats panel before removing it", async ({
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
            duration: 60,
            show_charts: true,
        });
        runtime.postGameStats({
            file: "fade-test.SC2Replay",
            mainPrestige: "Renegade Commander",
            allyPrestige: "Queen of Blades",
            comp: "Terran",
            player_stats: {
                army: { p1: [1, 2], p2: [2, 3], labels: ["0:00", "0:10"] },
                supply: { p1: [10, 20], p2: [8, 18], labels: ["0:00", "0:10"] },
                killed: { p1: [0, 5], p2: [0, 4], labels: ["0:00", "0:10"] },
                mining: {
                    p1: [200, 350],
                    p2: [180, 330],
                    labels: ["0:00", "0:10"],
                },
            },
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
        });
        runtime.showhide();
    });

    await expect(page.locator("#stats")).toBeVisible();

    await page.evaluate(() => {
        const runtime = window as typeof window & {
            showhide: () => void;
        };
        runtime.showhide();
    });

    await expect(page.locator("#stats")).toBeVisible();
    await expect(page.locator("#stats")).toHaveCSS("opacity", "0");

    await page.waitForTimeout(1100);

    await expect(page.locator("#stats")).not.toBeVisible();
});
