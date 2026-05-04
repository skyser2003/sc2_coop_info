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

async function installOverlayLevelMock(page: import("@playwright/test").Page) {
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

function buildReplayPayload(
    mainCommanderLevel: number,
    mainMasteryLevel: number,
    allyCommanderLevel: number,
    allyMasteryLevel: number,
): OverlayReplayPayload {
    return {
        file: "overlay-levels.SC2Replay",
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
        length: 100,
        "B+": 0,
        weekly: false,
        extension: false,
        mainCommanderLevel: mainCommanderLevel,
        allyCommanderLevel: allyCommanderLevel,
        mainMasteryLevel: mainMasteryLevel,
        allyMasteryLevel: allyMasteryLevel,
        mainMasteries: [0, 0, 0, 0, 0, 0],
        allyMasteries: [0, 0, 0, 0, 0, 0],
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

test("result overlay shows either commander level or mastery level by level status", async ({
    page,
}) => {
    await installOverlayLevelMock(page);
    await page.goto("/#/overlay", { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#com1", { state: "attached" });

    await postReplay(page, buildReplayPayload(14, 456, 15, 1000));

    await expect(page.locator("#com1")).toContainText("Raynor");
    await expect(page.locator("#com1")).toContainText("Lv 14");
    await expect(page.locator("#com1")).not.toContainText("M 456");
    await expect(page.locator("#com2")).toContainText("Kerrigan");
    await expect(page.locator("#com2")).not.toContainText("Lv 15");
    await expect(page.locator("#com2")).toContainText("M 1000");

    await postReplay(page, buildReplayPayload(15, 321, 9, 222));

    await expect(page.locator("#com1")).not.toContainText("Lv 15");
    await expect(page.locator("#com1")).toContainText("M 321");
    await expect(page.locator("#com2")).toContainText("Lv 9");
    await expect(page.locator("#com2")).not.toContainText("M 222");

    await postReplay(page, buildReplayPayload(12, 0, 15, 0));

    await expect(page.locator("#com1")).toContainText("Lv 12");
    await expect(page.locator("#com1")).not.toContainText("M 0");
    await expect(page.locator("#com2")).toContainText("Lv 15");
    await expect(page.locator("#com2")).not.toContainText("M 0");
});
