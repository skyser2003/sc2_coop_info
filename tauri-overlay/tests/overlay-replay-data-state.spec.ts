import { expect, test, type Page } from "@playwright/test";
import type {
    OverlayActionResponse,
    OverlayInitColorsDurationPayload,
    OverlayReplayPayload,
} from "../src/bindings/overlay";

test.describe.configure({ timeout: 60_000 });

type ReplayDataStatePayload = {
    active: boolean;
};

type OverlayInvokeRequest = {
    action?: string;
    event?: string;
    handler?: number;
    payload?: ReplayDataStatePayload;
};

type OverlayEventPayload =
    | OverlayInitColorsDurationPayload
    | OverlayReplayPayload
    | Record<string, never>;

type OverlayMockEvent = {
    event: string;
    id: number;
    payload: OverlayEventPayload;
};

type OverlayMockCallback = (payload: OverlayMockEvent) => void;

type OverlayConfigPayload = {
    status: string;
    settings: Record<string, never>;
    active_settings: Record<string, never>;
    randomizer_catalog: {
        prestige_names: Record<string, never>;
        mutators: [];
        brutal_plus: [];
    };
    monitor_catalog: [];
};

type OverlayInvokeResponse =
    | number
    | null
    | OverlayActionResponse
    | OverlayConfigPayload;

type OverlayTauriInternals = {
    invoke: (
        command: string,
        request?: OverlayInvokeRequest,
    ) => Promise<OverlayInvokeResponse>;
    transformCallback: (callback: OverlayMockCallback) => number;
    unregisterCallback: (id: number) => void;
};

declare global {
    interface Window {
        __SCO_REPLAY_DATA_STATE_REQUESTS__: boolean[];
        postGameStats?: (payload: OverlayReplayPayload) => void;
    }
}

class OverlayReplayDataStateTestOps {
    public static async installMock(page: Page): Promise<void> {
        await page.addInitScript(() => {
            const callbacks = new Map<number, OverlayMockCallback>();
            let nextCallbackId = 1;
            let nextEventListenerId = 1;

            const configPayload: OverlayConfigPayload = {
                status: "ok",
                settings: {},
                active_settings: {},
                randomizer_catalog: {
                    prestige_names: {},
                    mutators: [],
                    brutal_plus: [],
                },
                monitor_catalog: [],
            };

            const actionResponse: OverlayActionResponse = {
                status: "ok",
                result: { ok: true, path: null },
                message: "ok",
                randomizer: null,
            };

            const internals: OverlayTauriInternals = {
                transformCallback: (callback: OverlayMockCallback): number => {
                    const id = nextCallbackId;
                    nextCallbackId += 1;
                    callbacks.set(id, callback);
                    return id;
                },
                unregisterCallback: (id: number): void => {
                    callbacks.delete(id);
                },
                invoke: async (
                    command: string,
                    request?: OverlayInvokeRequest,
                ): Promise<OverlayInvokeResponse> => {
                    if (command === "plugin:event|listen") {
                        return nextEventListenerId++;
                    }
                    if (command === "plugin:event|unlisten") {
                        return null;
                    }
                    if (command === "config_get") {
                        return configPayload;
                    }
                    if (command === "config_action") {
                        if (
                            request?.action === "overlay_replay_data_state" &&
                            request.payload != null
                        ) {
                            window.__SCO_REPLAY_DATA_STATE_REQUESTS__.push(
                                request.payload.active,
                            );
                        }
                        return actionResponse;
                    }

                    throw new Error(`Unexpected command: ${command}`);
                },
            };

            window.__SCO_REPLAY_DATA_STATE_REQUESTS__ = [];
            window.__TAURI_INTERNALS__ = internals;
        });
    }

    public static replayPayload(): OverlayReplayPayload {
        return {
            file: "startup-navigation.SC2Replay",
            map_name: "Chain of Ascension",
            main: "Player One",
            ally: "Player Two",
            mainCommander: "Raynor",
            allyCommander: "Kerrigan",
            mainAPM: 100,
            allyAPM: 90,
            mainkills: 10,
            allykills: 20,
            result: "Victory",
            difficulty: "Brutal",
            enemy: "Terran",
            length: 100,
            "B+": 0,
            weekly: false,
            extension: false,
            mainCommanderLevel: 15,
            allyCommanderLevel: 15,
            mainMasteryLevel: 90,
            allyMasteryLevel: 90,
            mainMasteries: [30, 0, 30, 0, 30, 0],
            allyMasteries: [0, 30, 0, 30, 0, 30],
            mainUnits: {
                Marine: [5, 0, 10, 1],
            },
            allyUnits: {
                Zergling: [8, 0, 20, 1],
            },
            amon_units: {},
            mainIcons: {},
            allyIcons: {},
            mutators: [],
            bonus: [],
            player_stats: {},
            mainPrestige: "Renegade Commander",
            allyPrestige: "Queen of Blades",
            Victory: 1,
            Defeat: 0,
            fastest: false,
            comp: "Terran",
        };
    }

    public static async waitForOverlayBridge(page: Page): Promise<void> {
        await page.waitForFunction(
            () => typeof window.postGameStats === "function",
        );
    }

    public static async clearReplayDataStateRequests(
        page: Page,
    ): Promise<void> {
        await page.evaluate(() => {
            window.__SCO_REPLAY_DATA_STATE_REQUESTS__ = [];
        });
    }

    public static async postReplayPayload(
        page: Page,
        payload: OverlayReplayPayload,
    ): Promise<void> {
        await page.evaluate((nextPayload: OverlayReplayPayload) => {
            window.postGameStats?.(nextPayload);
        }, payload);
    }

    public static async replayDataStateRequests(
        page: Page,
    ): Promise<boolean[]> {
        return page.evaluate(() => {
            return [...window.__SCO_REPLAY_DATA_STATE_REQUESTS__];
        });
    }
}

test("manual replay payload marks replay data active without an inactive transition", async ({
    page,
}) => {
    await OverlayReplayDataStateTestOps.installMock(page);
    await page.goto("/#/overlay", { waitUntil: "domcontentloaded" });
    await OverlayReplayDataStateTestOps.waitForOverlayBridge(page);
    await OverlayReplayDataStateTestOps.clearReplayDataStateRequests(page);

    await OverlayReplayDataStateTestOps.postReplayPayload(
        page,
        OverlayReplayDataStateTestOps.replayPayload(),
    );

    await page.waitForFunction(
        () => window.__SCO_REPLAY_DATA_STATE_REQUESTS__.length > 0,
    );
    await page.waitForTimeout(50);

    const requests =
        await OverlayReplayDataStateTestOps.replayDataStateRequests(page);
    expect(requests).toContain(true);
    expect(requests).not.toContain(false);
});
