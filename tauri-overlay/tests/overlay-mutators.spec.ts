import { expect, test } from "@playwright/test";
import type {
    OverlayInitColorsDurationPayload,
    OverlayReplayPayload,
} from "../src/bindings/overlay";

test.describe.configure({ timeout: 60_000 });

async function installOverlayMutatorMock(
    page: import("@playwright/test").Page,
) {
    await page.addInitScript(() => {
        const listeners = new Map<string, number[]>();
        type MockEventPayload =
            | OverlayInitColorsDurationPayload
            | OverlayReplayPayload;
        type MockEvent = {
            event: string;
            id: number;
            payload: MockEventPayload;
        };
        type EmitMockEvent = (
            eventName: string,
            payload: MockEventPayload,
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

                if (command !== "config_request") {
                    throw new Error(`Unexpected command: ${command}`);
                }

                if (request.method === "GET" && request.path === "/config") {
                    return {
                        status: "ok",
                        randomizer_catalog: {
                            commander_mastery: {},
                            prestige_names: {},
                        },
                    };
                }

                if (
                    request.method === "POST" &&
                    request.path === "/config/action"
                ) {
                    return {
                        status: "ok",
                        result: { ok: true },
                        message: "ok",
                    };
                }

                throw new Error(
                    `Unexpected request: ${request.method} ${request.path}`,
                );
            },
        };

        (
            window as typeof window & {
                __emitMockEvent?: EmitMockEvent;
            }
        ).__emitMockEvent = (eventName: string, payload: MockEventPayload) => {
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

test("overlay renders all mutator icons for replay payloads", async ({
    page,
}) => {
    await installOverlayMutatorMock(page);
    await page.goto("/#/overlay", { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#mutators", { state: "attached" });

    await page.evaluate(() => {
        const runtime = window as typeof window & {
            __emitMockEvent?: (
                eventName: string,
                payload:
                    | OverlayInitColorsDurationPayload
                    | OverlayReplayPayload,
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

        runtime.__emitMockEvent?.("sco://overlay-replay-payload", {
            file: "overlay-mutators.SC2Replay",
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
            extension: true,
            mainCommanderLevel: 15,
            allyCommanderLevel: 15,
            mainMasteries: [0, 0, 0, 0, 0, 0],
            allyMasteries: [0, 0, 0, 0, 0, 0],
            mainUnits: {},
            allyUnits: {},
            amon_units: {},
            mainIcons: {},
            allyIcons: {},
            mutators: ["Moment of Silence", "Barrier"],
            bonus: [],
            player_stats: null,
            mainPrestige: "",
            allyPrestige: "",
            comp: "Terran",
        });
    });

    await expect(page.locator("#mutators img")).toHaveCount(2);
    await expect(page.locator("#mutators img").first()).toHaveAttribute(
        "src",
        /Mutator Icons\/Moment of Silence\.png$/,
    );
    await expect(page.locator("#mutators img").nth(1)).toHaveAttribute(
        "src",
        /Mutator Icons\/Barrier\.png$/,
    );
});
