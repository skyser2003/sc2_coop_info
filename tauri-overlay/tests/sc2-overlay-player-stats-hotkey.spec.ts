import { expect, test, type Page } from "@playwright/test";
import type {
    FirstWinBonusTimerPayload,
    OverlayInitColorsDurationPayload,
    OverlayLanguagePreviewPayload,
    OverlayPlayerStatsPayload,
} from "../src/bindings/overlay";

test.describe.configure({ timeout: 60_000 });

type Sc2OverlayConfigPayload = {
    active_settings: {
        language: string;
    };
};

type Sc2OverlayEventPayload =
    | FirstWinBonusTimerPayload
    | OverlayInitColorsDurationPayload
    | OverlayLanguagePreviewPayload
    | OverlayPlayerStatsPayload;

type Sc2OverlayMockEvent = {
    event: string;
    id: number;
    payload: Sc2OverlayEventPayload;
};

type Sc2OverlayMockCallback = (payload: Sc2OverlayMockEvent) => void;

type Sc2OverlayInvokeRequest = {
    event?: string;
    eventId?: number;
    handler?: number;
};

type Sc2OverlayInvokeResponse = number | null | Sc2OverlayConfigPayload;

type Sc2OverlayRegisteredListener = {
    eventName: string;
    handler: number;
};

type Sc2OverlayTauriInternals = {
    invoke: (
        command: string,
        request?: Sc2OverlayInvokeRequest,
    ) => Promise<Sc2OverlayInvokeResponse>;
    transformCallback: (callback: Sc2OverlayMockCallback) => number;
    unregisterCallback: (id: number) => void;
};

declare global {
    interface Window {
        __emitMockSc2OverlayEvent?: (
            eventName: string,
            payload: Sc2OverlayEventPayload,
        ) => void;
    }
}

class Sc2OverlayPlayerStatsHotkeyTestOps {
    public static async installMock(page: Page): Promise<void> {
        await page.addInitScript(() => {
            const listeners = new Map<string, number[]>();
            const registeredListeners = new Map<
                number,
                Sc2OverlayRegisteredListener
            >();
            const callbacks = new Map<number, Sc2OverlayMockCallback>();
            let nextCallbackId = 1;
            let nextEventListenerId = 1;

            const configPayload: Sc2OverlayConfigPayload = {
                active_settings: {
                    language: "en",
                },
            };

            const internals: Sc2OverlayTauriInternals = {
                transformCallback: (
                    callback: Sc2OverlayMockCallback,
                ): number => {
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
                    request?: Sc2OverlayInvokeRequest,
                ): Promise<Sc2OverlayInvokeResponse> => {
                    if (command === "plugin:event|listen") {
                        const eventName = request?.event ?? "";
                        const handler = request?.handler ?? 0;
                        const currentListeners = listeners.get(eventName) ?? [];
                        currentListeners.push(handler);
                        listeners.set(eventName, currentListeners);
                        const listenerId = nextEventListenerId;
                        nextEventListenerId += 1;
                        registeredListeners.set(listenerId, {
                            eventName,
                            handler,
                        });
                        return listenerId;
                    }

                    if (command === "plugin:event|unlisten") {
                        const listenerId = request?.eventId ?? 0;
                        const registered = registeredListeners.get(listenerId);
                        if (registered != null) {
                            const currentListeners =
                                listeners.get(registered.eventName) ?? [];
                            listeners.set(
                                registered.eventName,
                                currentListeners.filter(
                                    (handler) => handler !== registered.handler,
                                ),
                            );
                            registeredListeners.delete(listenerId);
                        }
                        return null;
                    }

                    if (command === "config_get") {
                        return configPayload;
                    }

                    if (command === "config_request") {
                        return configPayload;
                    }

                    throw new Error(`Unexpected command: ${command}`);
                },
            };

            window.__TAURI_INTERNALS__ = internals;
            window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener: (
                    eventName: string,
                    listenerId: number,
                ): void => {
                    const registered = registeredListeners.get(listenerId);
                    if (
                        registered == null ||
                        registered.eventName !== eventName
                    ) {
                        return;
                    }

                    const currentListeners = listeners.get(eventName) ?? [];
                    listeners.set(
                        eventName,
                        currentListeners.filter(
                            (handler) => handler !== registered.handler,
                        ),
                    );
                    registeredListeners.delete(listenerId);
                },
            };
            window.__emitMockSc2OverlayEvent = (
                eventName: string,
                payload: Sc2OverlayEventPayload,
            ): void => {
                for (const callbackId of listeners.get(eventName) ?? []) {
                    callbacks.get(callbackId)?.({
                        event: eventName,
                        id: callbackId,
                        payload,
                    });
                }
            };
        });
    }

    public static playerStatsPayload(
        playerName: string,
    ): OverlayPlayerStatsPayload {
        return {
            data: {
                [playerName]: {
                    kind: "stats",
                    wins: 2,
                    losses: 1,
                    apm: 123,
                    commander: "Raynor",
                    frequency: 1,
                    kills: 0.42,
                    last_seen_relative: "today",
                },
            },
        };
    }

    public static async emitPlayerStatsHotkey(
        page: Page,
        payload: OverlayPlayerStatsPayload,
    ): Promise<void> {
        await page.evaluate((nextPayload: OverlayPlayerStatsPayload) => {
            window.__emitMockSc2OverlayEvent?.(
                "sco://overlay-show-hide-player-stats",
                nextPayload,
            );
        }, payload);
    }

    public static async emitFirstWinBonusTimer(
        page: Page,
        payload: FirstWinBonusTimerPayload,
    ): Promise<void> {
        await page.evaluate((nextPayload: FirstWinBonusTimerPayload) => {
            window.__emitMockSc2OverlayEvent?.(
                "sco://overlay-first-win-bonus-timer",
                nextPayload,
            );
        }, payload);
    }
}

test("player stats hotkey shows from hidden state", async ({ page }) => {
    await Sc2OverlayPlayerStatsHotkeyTestOps.installMock(page);
    await page.goto("/#/sc2-overlay", { waitUntil: "domcontentloaded" });

    const playerStats = page.locator("#playerstats");
    await expect(playerStats).toBeAttached();

    await Sc2OverlayPlayerStatsHotkeyTestOps.emitPlayerStatsHotkey(
        page,
        Sc2OverlayPlayerStatsHotkeyTestOps.playerStatsPayload("First Ally"),
    );
    await expect(playerStats).toBeVisible();
    await expect(playerStats).toContainText("First Ally");
});

test("player stats hotkey hides visible stats on the next press", async ({
    page,
}) => {
    await Sc2OverlayPlayerStatsHotkeyTestOps.installMock(page);
    await page.goto("/#/sc2-overlay", { waitUntil: "domcontentloaded" });

    const playerStats = page.locator("#playerstats");
    await expect(playerStats).toBeAttached();

    await Sc2OverlayPlayerStatsHotkeyTestOps.emitPlayerStatsHotkey(
        page,
        Sc2OverlayPlayerStatsHotkeyTestOps.playerStatsPayload("Toggle Ally"),
    );
    await expect(playerStats).toBeVisible();
    await expect(playerStats).toContainText("Toggle Ally");

    await Sc2OverlayPlayerStatsHotkeyTestOps.emitPlayerStatsHotkey(
        page,
        Sc2OverlayPlayerStatsHotkeyTestOps.playerStatsPayload("Toggle Ally"),
    );

    await expect(playerStats).toBeHidden();
});

test("player stats hotkey returns to first win bonus timer when configured visible", async ({
    page,
}) => {
    await Sc2OverlayPlayerStatsHotkeyTestOps.installMock(page);
    await page.goto("/#/sc2-overlay", { waitUntil: "domcontentloaded" });

    const playerStats = page.locator("#playerstats");
    const firstWinBonusTimer = page.locator("#firstWinBonusTimer");
    await expect(playerStats).toBeAttached();
    await expect(firstWinBonusTimer).toBeAttached();

    await Sc2OverlayPlayerStatsHotkeyTestOps.emitFirstWinBonusTimer(page, {
        visible: true,
        available: true,
        seconds_until_available: 0,
    });
    await expect(firstWinBonusTimer).toBeVisible();
    await expect(firstWinBonusTimer).toHaveCSS("transition-duration", "1s");

    await Sc2OverlayPlayerStatsHotkeyTestOps.emitFirstWinBonusTimer(page, {
        visible: false,
        available: true,
        seconds_until_available: 0,
    });
    await expect(firstWinBonusTimer).toHaveCSS("transition-duration", "1s");
    await expect(firstWinBonusTimer).toHaveCSS("opacity", "0");

    await Sc2OverlayPlayerStatsHotkeyTestOps.emitFirstWinBonusTimer(page, {
        visible: true,
        available: true,
        seconds_until_available: 0,
    });
    await expect(firstWinBonusTimer).toHaveCSS("transition-duration", "1s");
    await expect(firstWinBonusTimer).toHaveCSS("opacity", "1");

    await Sc2OverlayPlayerStatsHotkeyTestOps.emitPlayerStatsHotkey(
        page,
        Sc2OverlayPlayerStatsHotkeyTestOps.playerStatsPayload("Toggle Ally"),
    );
    await expect(playerStats).toBeVisible();
    await expect(playerStats).toContainText("Toggle Ally");
    await expect(firstWinBonusTimer).toHaveCSS("opacity", "0");

    await Sc2OverlayPlayerStatsHotkeyTestOps.emitPlayerStatsHotkey(
        page,
        Sc2OverlayPlayerStatsHotkeyTestOps.playerStatsPayload("Toggle Ally"),
    );

    await expect(playerStats).toBeHidden();
    await expect(firstWinBonusTimer).toHaveCSS("opacity", "1");
    await expect(firstWinBonusTimer).toHaveCSS("transition-duration", "0s");
});
