import { expect, test } from "@playwright/test";
import type {
    OverlayInitColorsDurationPayload,
    OverlayReplayPayload,
} from "../src/bindings/overlay";

test.describe.configure({ timeout: 60_000 });

async function installOverlayEventMock(page: import("@playwright/test").Page) {
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
    file: string,
    mainkills: number,
    allykills: number,
    options?: {
        newReplay?: boolean;
    },
): OverlayReplayPayload {
    return {
        file,
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
        extension: false,
        "B+": 0,
        mainkills,
        allykills,
        mainIcons: {},
        mainMasteries: [30, 0, 30, 0, 30, 0],
        mainUnits: {
            Marine: [5, 0, mainkills, 1],
        },
        allyIcons: {},
        allyMasteries: [0, 30, 0, 30, 0, 30],
        allyUnits: {
            Zergling: [8, 0, allykills, 1],
        },
        amon_units: {},
        ...(options?.newReplay === undefined
            ? {}
            : { newReplay: options.newReplay }),
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
                payload:
                    | OverlayInitColorsDurationPayload
                    | OverlayReplayPayload
                    | Record<string, never>,
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

test("kill ratio bar keeps the previous replay ratio as the next animation start", async ({
    page,
}) => {
    await installOverlayEventMock(page);
    await page.goto("/#/overlay", { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#killbar1", { state: "attached" });

    await postReplay(
        page,
        buildReplayPayload("first.SC2Replay", 10, 20, { newReplay: true }),
    );

    await expect
        .poll(
            () =>
                page
                    .locator("#killbar1")
                    .evaluate(
                        (element) => (element as HTMLElement).style.width,
                    ),
            { timeout: 2_000 },
        )
        .toBe("33%");
    await expect
        .poll(
            () =>
                page
                    .locator("#killbar2")
                    .evaluate(
                        (element) => (element as HTMLElement).style.width,
                    ),
            { timeout: 2_000 },
        )
        .toBe("67%");

    await postReplay(
        page,
        buildReplayPayload("second.SC2Replay", 30, 10, { newReplay: true }),
    );

    await page.waitForTimeout(100);

    await expect(
        page.locator("#killbar1").evaluate((element) => {
            return (element as HTMLElement).style.width;
        }),
    ).resolves.toBe("33%");
    await expect(
        page.locator("#killbar2").evaluate((element) => {
            return (element as HTMLElement).style.width;
        }),
    ).resolves.toBe("67%");

    await expect
        .poll(
            () =>
                page
                    .locator("#killbar1")
                    .evaluate(
                        (element) => (element as HTMLElement).style.width,
                    ),
            { timeout: 2_000 },
        )
        .toBe("75%");
    await expect
        .poll(
            () =>
                page
                    .locator("#killbar2")
                    .evaluate(
                        (element) => (element as HTMLElement).style.width,
                    ),
            { timeout: 2_000 },
        )
        .toBe("25%");
});

test("manual replay navigation updates the kill ratio bar without post-game delay", async ({
    page,
}) => {
    await installOverlayEventMock(page);
    await page.goto("/#/overlay", { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#killbar1", { state: "attached" });

    await postReplay(
        page,
        buildReplayPayload("first.SC2Replay", 10, 20, { newReplay: true }),
    );

    await expect
        .poll(
            () =>
                page
                    .locator("#killbar1")
                    .evaluate(
                        (element) => (element as HTMLElement).style.width,
                    ),
            { timeout: 2_000 },
        )
        .toBe("33%");
    await expect
        .poll(
            () =>
                page
                    .locator("#killbar2")
                    .evaluate(
                        (element) => (element as HTMLElement).style.width,
                    ),
            { timeout: 2_000 },
        )
        .toBe("67%");

    await postReplay(page, buildReplayPayload("second.SC2Replay", 30, 10));

    await page.waitForTimeout(100);

    await expect(
        page.locator("#killbar1").evaluate((element) => {
            return (element as HTMLElement).style.width;
        }),
    ).resolves.toBe("75%");
    await expect(
        page.locator("#killbar2").evaluate((element) => {
            return (element as HTMLElement).style.width;
        }),
    ).resolves.toBe("25%");
});
