import { expect, test } from "@playwright/test";
import type {
    OverlayInitColorsDurationPayload,
    OverlayReplayPayload,
} from "../src/bindings/overlay";

test.describe.configure({ timeout: 60_000 });

type OverlayEventPayload =
    | OverlayInitColorsDurationPayload
    | OverlayReplayPayload
    | Record<string, never>;

async function installOverlayUnitTableMock(
    page: import("@playwright/test").Page,
) {
    await page.addInitScript(() => {
        const listeners = new Map<string, number[]>();
        type OverlayEventPayload =
            | OverlayInitColorsDurationPayload
            | OverlayReplayPayload
            | Record<string, never>;
        type MockEvent = {
            event: string;
            id: number;
            payload: OverlayEventPayload;
        };
        type EmitMockEvent = (
            eventName: string,
            payload: OverlayEventPayload,
        ) => void;

        const callbacks = new Map<number, (payload: MockEvent) => void>();
        let nextCallbackId = 1;
        let nextEventListenerId = 1;

        window.__TAURI_INTERNALS__ = {
            transformCallback: (callback: (payload: MockEvent) => void) => {
                const id = nextCallbackId++;
                callbacks.set(id, callback);
                return id;
            },
            unregisterCallback: (id: number) => {
                callbacks.delete(id);
            },
            invoke: async (
                command: string,
                request: {
                    path?: string;
                    method?: string;
                    event?: string;
                    handler?: number;
                },
            ) => {
                if (command === "plugin:event|listen") {
                    const eventName = request.event ?? "";
                    const handler = request.handler ?? 0;
                    const current = listeners.get(eventName) || [];
                    current.push(handler);
                    listeners.set(eventName, current);
                    return nextEventListenerId++;
                }

                if (command === "plugin:event|unlisten") {
                    return null;
                }

                if (
                    command === "config_get" ||
                    (command === "config_request" &&
                        request.method === "GET" &&
                        request.path === "/config")
                ) {
                    return {
                        status: "ok",
                        randomizer_catalog: {
                            commander_mastery: {},
                            prestige_names: {},
                        },
                    };
                }

                if (
                    command === "config_action" ||
                    (command === "config_request" &&
                        request.method === "POST" &&
                        request.path === "/config/action")
                ) {
                    return {
                        status: "ok",
                        result: { ok: true },
                        message: "ok",
                    };
                }

                throw new Error(`Unexpected command: ${command}`);
            },
        };

        (
            window as typeof window & {
                __emitMockEvent?: EmitMockEvent;
            }
        ).__emitMockEvent = (
            eventName: string,
            payload: OverlayEventPayload,
        ) => {
            for (const callbackId of listeners.get(eventName) || []) {
                callbacks.get(callbackId)?.({
                    event: eventName,
                    id: callbackId,
                    payload,
                });
            }
        };
    });
}

function buildReplayPayload(): OverlayReplayPayload {
    return {
        file: "unit-table.SC2Replay",
        map_name: "Chain of Ascension",
        main: "Player One",
        ally: "Player Two",
        mainCommander: "Raynor",
        allyCommander: "Kerrigan",
        mainAPM: 100,
        allyAPM: 90,
        mainkills: 30,
        allykills: 20,
        result: "Victory",
        difficulty: "Brutal",
        length: 100,
        "B+": 0,
        weekly: false,
        extension: false,
        mainCommanderLevel: 15,
        allyCommanderLevel: 15,
        mainMasteries: [30, 0, 30, 0, 30, 0],
        allyMasteries: [0, 30, 0, 30, 0, 30],
        mainUnits: {
            Marine: [9, 4, 12, 0.4],
            Marauder: [6, 1, 18, 0.6],
            Firebat: [2, 1, 0, 0],
        },
        allyUnits: {
            Hydralisk: [4, 2, 11, 0.55],
            Zergling: [12, 7, 9, 0.45],
        },
        amon_units: {
            Marine: [8, 6, 7, 0.7],
            Marauder: [5, 3, 3, 0.3],
        },
        mainIcons: {},
        allyIcons: {},
        mutators: [],
        bonus: [],
        mainPrestige: "Renegade Commander",
        allyPrestige: "Queen of Blades",
        Victory: 1,
        Defeat: 0,
        fastest: false,
        comp: "Terran",
    };
}

async function postReplay(
    page: import("@playwright/test").Page,
    payload: OverlayReplayPayload,
) {
    await page.evaluate((nextPayload: OverlayReplayPayload) => {
        const runtime = window as typeof window & {
            __emitMockEvent?: (
                eventName: string,
                payload: OverlayEventPayload,
            ) => void;
        };

        runtime.__emitMockEvent?.("sco://overlay-init-colors-duration", {
            colors: [null, null, null, null],
            duration: 60,
            show_charts: false,
            show_session: false,
            hide_nicknames_in_overlay: false,
            session_victory: 0,
            session_defeat: 0,
            language: "en",
        });
        runtime.__emitMockEvent?.("sco://overlay-replay-payload", nextPayload);
    }, payload);
}

test("replay kill unit ranking uses semantic tables", async ({ page }) => {
    await installOverlayUnitTableMock(page);
    await page.goto("/#/overlay", { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#stats", { state: "attached" });

    await postReplay(page, buildReplayPayload());

    const mainTable = page.locator("#CMunits1 table.units-table");
    const allyTable = page.locator("#CMunits2 table.units-table");
    const amonTable = page.locator("#CMunits3 table.units-table");

    await expect(mainTable).toBeVisible();
    await expect(allyTable).toBeVisible();
    await expect(amonTable).toBeVisible();

    await expect(mainTable.locator("thead th")).toHaveText([
        "Unit",
        "kills",
        "Created",
        "Lost",
    ]);
    await expect(mainTable.locator("tbody tr")).toHaveCount(2);
    await expect(
        mainTable.locator("tbody tr").nth(0).locator("td").nth(0),
    ).toHaveText("Marauder");
    await expect(
        mainTable.locator("tbody tr").nth(0).locator("td").nth(1),
    ).toHaveText("60%");
    await expect(
        mainTable.locator("tbody tr").nth(0).locator("td").nth(2),
    ).toHaveText("18");
    await expect(
        mainTable.locator("tbody tr").nth(1).locator("td").nth(0),
    ).toHaveText("Marine");
    await expect(
        mainTable.locator("tbody tr").nth(0).locator(".units-table-name-bg"),
    ).toBeVisible();

    await expect(allyTable.locator("tbody tr")).toHaveCount(2);
    await expect(amonTable.locator("tbody tr")).toHaveCount(2);
    await expect(page.locator("#stats .unitline")).toHaveCount(0);

    const geometry = await page.evaluate(() => {
        function readWidth(selector: string): number {
            const element = document.querySelector(selector);
            if (!(element instanceof HTMLElement)) {
                throw new Error(`Missing element: ${selector}`);
            }
            return element.getBoundingClientRect().width;
        }

        return {
            mainTableWidth: readWidth("#CMunits1 table.units-table"),
            allyTableWidth: readWidth("#CMunits2 table.units-table"),
            amonTableWidth: readWidth("#CMunits3 table.units-table"),
            mainNameWidth: readWidth(
                "#CMunits1 tbody tr:first-child td.units-table-name",
            ),
            allyNameWidth: readWidth(
                "#CMunits2 tbody tr:first-child td.units-table-name",
            ),
            amonNameWidth: readWidth(
                "#CMunits3 tbody tr:first-child td.units-table-name",
            ),
            mainKillPercentWidth: readWidth(
                "#CMunits1 tbody tr:first-child td.units-table-kill-percent",
            ),
            mainKillCountWidth: readWidth(
                "#CMunits1 tbody tr:first-child td.units-table-kill-count",
            ),
            killPercentTextAlign: window.getComputedStyle(
                document.querySelector(
                    "#CMunits1 tbody tr:first-child td.units-table-kill-percent",
                ) as Element,
            ).textAlign,
            killPercentPaddingRight: window.getComputedStyle(
                document.querySelector(
                    "#CMunits1 tbody tr:first-child .units-table-kill-percent-value",
                ) as Element,
            ).paddingRight,
            killCountTextAlign: window.getComputedStyle(
                document.querySelector(
                    "#CMunits1 tbody tr:first-child td.units-table-kill-count",
                ) as Element,
            ).textAlign,
            killCountPaddingLeft: window.getComputedStyle(
                document.querySelector(
                    "#CMunits1 tbody tr:first-child .units-table-kill-count-value",
                ) as Element,
            ).paddingLeft,
            killCountBorderLeftWidth: window.getComputedStyle(
                document.querySelector(
                    "#CMunits1 tbody tr:first-child td.units-table-kill-count",
                ) as Element,
            ).borderLeftWidth,
            marauderBgBottom: (
                document.querySelector(
                    "#CMunits1 tbody tr:first-child .units-table-name-bg",
                ) as HTMLElement
            ).getBoundingClientRect().bottom,
            marineBgTop: (
                document.querySelector(
                    "#CMunits1 tbody tr:nth-child(2) .units-table-name-bg",
                ) as HTMLElement
            ).getBoundingClientRect().top,
            marauderBgWidth: readWidth(
                "#CMunits1 tbody tr:first-child .units-table-name-bg",
            ),
            marineBgWidth: readWidth(
                "#CMunits1 tbody tr:nth-child(2) .units-table-name-bg",
            ),
        };
    });

    expect(
        Math.abs(geometry.mainTableWidth - geometry.allyTableWidth),
    ).toBeLessThanOrEqual(1);
    expect(
        Math.abs(geometry.mainTableWidth - geometry.amonTableWidth),
    ).toBeLessThanOrEqual(1);
    expect(
        Math.abs(geometry.mainNameWidth - geometry.allyNameWidth),
    ).toBeLessThanOrEqual(1);
    expect(
        Math.abs(geometry.mainNameWidth - geometry.amonNameWidth),
    ).toBeLessThanOrEqual(1);
    expect(geometry.mainKillPercentWidth).toBeGreaterThan(0);
    expect(geometry.mainKillCountWidth).toBeGreaterThan(0);
    expect(geometry.killPercentTextAlign).toBe("right");
    expect(Number.parseFloat(geometry.killPercentPaddingRight)).toBeGreaterThan(
        0,
    );
    expect(geometry.killCountTextAlign).toBe("left");
    expect(Number.parseFloat(geometry.killCountPaddingLeft)).toBeGreaterThan(0);
    expect(
        Number.parseFloat(geometry.killCountBorderLeftWidth),
    ).toBeGreaterThan(0);
    expect(
        geometry.marineBgTop - geometry.marauderBgBottom,
    ).toBeLessThanOrEqual(1);
    expect(geometry.marauderBgWidth).toBeGreaterThan(geometry.marineBgWidth);
});
