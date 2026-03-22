import { expect, test } from "@playwright/test";

test.describe.configure({ timeout: 60_000 });

test("show charts setting controls replay chart visibility in the overlay", async ({
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
            setShowChartsFromConfig: (show: boolean) => void;
            postGameStats: (data: Record<string, unknown>) => void;
        };

        runtime.initColorsDuration({
            colors: [null, null, null, null],
            duration: 60,
            show_charts: true,
        });
        runtime.postGameStats({
            file: "show-charts-test.SC2Replay",
            mainPrestige: "Renegade Commander",
            allyPrestige: "Queen of Blades",
            comp: "Terran",
            player_stats: {
                1: {
                    name: "Player One",
                    army: [1, 2, 3],
                    supply: [12, 20, 30],
                    killed: [0, 5, 9],
                    mining: [200, 325, 410],
                },
                2: {
                    name: "Player Two",
                    army: [2, 3, 4],
                    supply: [11, 18, 28],
                    killed: [0, 4, 8],
                    mining: [180, 300, 390],
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
    });

    await expect
        .poll(() =>
            page.locator("#charts").evaluate((element) => {
                return (element as HTMLElement).style.opacity;
            }),
        )
        .toBe("1");
    await expect
        .poll(() =>
            page.locator("#charts").evaluate((element) => {
                return (element as HTMLElement).style.display;
            }),
        )
        .toBe("block");

    await page.evaluate(() => {
        const runtime = window as typeof window & {
            setShowChartsFromConfig: (show: boolean) => void;
        };

        runtime.setShowChartsFromConfig(false);
    });

    await expect
        .poll(() =>
            page.locator("#charts").evaluate((element) => {
                return (element as HTMLElement).style.opacity;
            }),
        )
        .toBe("0");
    await expect
        .poll(() =>
            page.locator("#charts").evaluate((element) => {
                return (element as HTMLElement).style.display;
            }),
        )
        .toBe("none");
});
