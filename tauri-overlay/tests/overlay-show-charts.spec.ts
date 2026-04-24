import { expect, test } from "@playwright/test";
import type {
    OverlayInitColorsDurationPayload,
    OverlayReplayPayload,
} from "../src/bindings/overlay";

test.describe.configure({ timeout: 60_000 });

type OverlayEventPayload =
    | OverlayInitColorsDurationPayload
    | OverlayReplayPayload
    | boolean
    | Record<string, never>;

async function installOverlayShowChartsMock(
    page: import("@playwright/test").Page,
) {
    await page.addInitScript(() => {
        const listeners = new Map<string, number[]>();
        type OverlayEventPayload =
            | OverlayInitColorsDurationPayload
            | OverlayReplayPayload
            | boolean
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

        const emitMockEvent: EmitMockEvent = (eventName, payload) => {
            for (const callbackId of listeners.get(eventName) || []) {
                callbacks.get(callbackId)?.({
                    event: eventName,
                    id: callbackId,
                    payload,
                });
            }
        };

        (
            window as typeof window & {
                initColorsDuration?: (data: {
                    colors: [
                        string | null,
                        string | null,
                        string | null,
                        string | null,
                    ];
                    duration: number;
                    show_charts: boolean;
                }) => void;
                postGameStats?: (data: Record<string, unknown>) => void;
                setShowChartsFromConfig?: (show: boolean) => void;
            }
        ).initColorsDuration = (data) => {
            emitMockEvent("sco://overlay-init-colors-duration", {
                colors: data.colors,
                duration: data.duration,
                show_charts: data.show_charts,
                show_session: false,
                hide_nicknames_in_overlay: false,
                session_victory: 0,
                session_defeat: 0,
                language: "en",
            });
        };

        (
            window as typeof window & {
                postGameStats?: (data: Record<string, unknown>) => void;
            }
        ).postGameStats = (data) => {
            emitMockEvent(
                "sco://overlay-replay-payload",
                data as OverlayReplayPayload,
            );
        };

        (
            window as typeof window & {
                setShowChartsFromConfig?: (show: boolean) => void;
            }
        ).setShowChartsFromConfig = (show) => {
            emitMockEvent("sco://overlay-set-show-charts-from-config", show);
        };
    });
}

test("show charts setting controls replay chart visibility in the overlay", async ({
    page,
}) => {
    await installOverlayShowChartsMock(page);
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

test("charts render from semantic main and ally player stats", async ({
    page,
}) => {
    await installOverlayShowChartsMock(page);
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
            duration: 60,
            show_charts: true,
        });
        runtime.postGameStats({
            file: "semantic-chart-test.SC2Replay",
            mainPrestige: "Renegade Commander",
            allyPrestige: "Queen of Blades",
            comp: "Terran",
            player_stats: {
                1: {
                    name: "Player Two",
                    army: [2, 3, 4],
                    supply: [11, 18, 28],
                    killed: [0, 4, 8],
                    mining: [180, 300, 390],
                },
                2: {
                    name: "Player One",
                    army: [1, 2, 3],
                    supply: [12, 20, 30],
                    killed: [0, 5, 9],
                    mining: [200, 325, 410],
                },
            },
            mainPlayerStats: {
                name: "Player One",
                army: [1, 2, 3],
                supply: [12, 20, 30],
                killed: [0, 5, 9],
                mining: [200, 325, 410],
            },
            allyPlayerStats: {
                name: "Player Two",
                army: [2, 3, 4],
                supply: [11, 18, 28],
                killed: [0, 4, 8],
                mining: [180, 300, 390],
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
                return (element as HTMLElement).style.display;
            }),
        )
        .toBe("block");
    await expect
        .poll(() =>
            page.locator("#charts").evaluate((element) => {
                return (element as HTMLElement).style.opacity;
            }),
        )
        .toBe("1");
});
