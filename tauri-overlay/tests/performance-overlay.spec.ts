import { expect, test } from "@playwright/test";

async function installTauriMock(
    page: import("@playwright/test").Page,
    options?: {
        performanceShow?: boolean;
    },
) {
    await page.addInitScript((initOptions?: { performanceShow?: boolean }) => {
        const cloneJson = (value: unknown) => JSON.parse(JSON.stringify(value));
        let settings = {
            account_folder: "fixtures/accounts",
            performance_show: true,
            performance_hotkey: "Ctrl+Alt+P",
            performance_processes: ["SC2_x64.exe", "SC2.exe"],
            rng_choices: {},
        };
        if (typeof initOptions?.performanceShow === "boolean") {
            settings = {
                ...settings,
                performance_show: initOptions.performanceShow,
            };
        }
        let activeSettings = cloneJson(settings);
        type ConfigPayload = {
            status: "ok";
            settings: typeof settings;
            active_settings: typeof settings;
            randomizer_catalog: {
                commander_mastery: Record<string, never>;
                prestige_names: Record<string, never>;
            };
            monitor_catalog: Array<{ index: number; label: string }>;
        };
        type ConfigCommandRequest = {
            path?: string;
            method?: string;
            settings?: typeof settings;
            action?: string;
            persist?: boolean;
            body?: {
                settings?: typeof settings;
                action?: string;
                persist?: boolean;
            };
        };
        const listeners = new Map<
            string,
            Array<(event: { payload: unknown }) => void>
        >();
        const createConfigPayload = (): ConfigPayload => ({
            status: "ok",
            settings,
            active_settings: activeSettings,
            randomizer_catalog: {
                commander_mastery: {},
                prestige_names: {},
            },
            monitor_catalog: [{ index: 1, label: "1 - Primary Monitor" }],
        });
        const updateSettings = (
            nextSettings: typeof settings,
            persist: boolean,
        ): void => {
            activeSettings = cloneJson(nextSettings);
            if (persist) {
                settings = cloneJson(nextSettings);
                activeSettings = cloneJson(nextSettings);
            } else {
                (
                    window as typeof window & {
                        __SCO_CONFIG_APPLY_REQUESTS__?: Array<
                            Record<string, unknown>
                        >;
                    }
                ).__SCO_CONFIG_APPLY_REQUESTS__?.push(
                    cloneJson(activeSettings) as Record<string, unknown>,
                );
            }
        };
        (
            window as typeof window & {
                __SCO_CONFIG_APPLY_REQUESTS__?: Array<Record<string, unknown>>;
            }
        ).__SCO_CONFIG_APPLY_REQUESTS__ = [];

        window.__TAURI_INTERNALS__ = {
            invoke: async (
                command: string,
                request: ConfigCommandRequest = {},
            ) => {
                if (command === "config_get") {
                    return createConfigPayload();
                }
                if (command === "config_update") {
                    updateSettings(
                        request.settings ?? activeSettings,
                        request.persist !== false,
                    );
                    return createConfigPayload();
                }
                if (command === "config_action") {
                    return {
                        status: "ok",
                        result: { ok: true, path: null },
                        message: request.action || "ok",
                        randomizer: null,
                    };
                }
                if (command === "config_stats_action") {
                    return {
                        status: "ok",
                        result: { ok: true, path: null },
                        message: request.action || "ok",
                        stats: null,
                    };
                }
                if (command !== "config_request") {
                    throw new Error(`Unexpected command: ${command}`);
                }
                if (request.method === "GET" && request.path === "/config") {
                    return createConfigPayload();
                }
                if (request.method === "POST" && request.path === "/config") {
                    const nextSettings =
                        request.body?.settings || activeSettings;
                    updateSettings(
                        nextSettings,
                        request.body?.persist !== false,
                    );
                    return createConfigPayload();
                }
                if (
                    request.method === "POST" &&
                    request.path === "/config/action"
                ) {
                    return {
                        status: "ok",
                        result: { ok: true },
                        message: request.body?.action || "ok",
                    };
                }
                if (
                    request.method === "POST" &&
                    request.path === "/config/stats/action"
                ) {
                    return { status: "ok", message: "ok" };
                }
                throw new Error(
                    `Unexpected request: ${request.method} ${request.path}`,
                );
            },
            event: {
                listen: async (
                    eventName: string,
                    callback: (event: { payload: unknown }) => void,
                ) => {
                    const current = listeners.get(eventName) || [];
                    current.push(callback);
                    listeners.set(eventName, current);
                    return () => {
                        const next = (listeners.get(eventName) || []).filter(
                            (entry) => entry !== callback,
                        );
                        listeners.set(eventName, next);
                    };
                },
            },
        };

        (
            window as typeof window & {
                __emitMockEvent?: (eventName: string, payload: unknown) => void;
            }
        ).__emitMockEvent = (eventName: string, payload: unknown) => {
            if (
                eventName === "sco://performance-visibility" &&
                payload &&
                typeof payload === "object" &&
                "visible" in payload
            ) {
                settings = {
                    ...settings,
                    performance_show: Boolean(payload.visible),
                };
                activeSettings = {
                    ...activeSettings,
                    performance_show: Boolean(payload.visible),
                };
            }
            for (const callback of listeners.get(eventName) || []) {
                callback({ payload });
            }
        };
    }, options);
}

test.describe.configure({ timeout: 60_000 });

test("performance config tab matches the legacy control layout", async ({
    page,
}) => {
    await installTauriMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.getByRole("tab", { name: "Performance" }).click();

    await expect(page.getByText("Performance overlay:")).toBeVisible();
    await expect(
        page.getByText(
            "Shows performance overlay with CPU/RAM/Disk/Network usage for system and StarCraft II.",
        ),
    ).toBeVisible();
    await expect(
        page.getByRole("checkbox", { name: "Show performance overlay" }),
    ).toBeChecked();
    await expect(
        page.getByRole("button", { name: "Change overlay position" }),
    ).toBeVisible();
    await expect(page.locator('input[value="Ctrl+Alt+P"]')).toBeVisible();
    await expect(page.locator(".performance-tab-process-input")).toHaveValue(
        "SC2_x64.exe\nSC2.exe",
    );

    await page
        .locator(".performance-tab-process-input")
        .fill("foo.exe\nbar.exe");

    await expect(page.locator(".performance-tab-process-input")).toHaveValue(
        "foo.exe\nbar.exe",
    );
    await expect
        .poll(() =>
            page.evaluate(() => {
                const runtimeWindow = window as Window & {
                    __SCO_CONFIG_APPLY_REQUESTS__?: Array<
                        Record<string, unknown>
                    >;
                };
                const requests =
                    runtimeWindow.__SCO_CONFIG_APPLY_REQUESTS__ || [];
                return requests[requests.length - 1] || null;
            }),
        )
        .toMatchObject({
            performance_processes: ["foo.exe", "bar.exe"],
        });

    await page.evaluate(() => {
        (
            window as typeof window & {
                __scoSetPerformanceVisibility?: (visible: boolean) => void;
            }
        ).__scoSetPerformanceVisibility?.(false);
    });

    await expect(
        page.getByRole("checkbox", { name: "Show performance overlay" }),
    ).not.toBeChecked();
});

test("performance checkbox follows display state during reposition mode", async ({
    page,
}) => {
    await installTauriMock(page, { performanceShow: false });
    await page.goto("/", { waitUntil: "domcontentloaded" });

    await page.getByRole("tab", { name: "Performance" }).click();

    const checkbox = page.getByRole("checkbox", {
        name: "Show performance overlay",
    });
    const repositionButton = page.getByRole("button", {
        name: "Change overlay position",
    });

    await expect(checkbox).not.toBeChecked();

    await repositionButton.click();
    await expect(checkbox).toBeChecked();

    await repositionButton.click();
    await expect(checkbox).not.toBeChecked();
});

test("performance overlay exposes edit mode header and renders pushed stats", async ({
    page,
}) => {
    await page.goto("/#/performance", { waitUntil: "domcontentloaded" });
    await page.waitForFunction(
        () =>
            typeof (
                window as typeof window & {
                    updatePerformanceStats?: unknown;
                    setPerformanceEditMode?: unknown;
                }
            ).updatePerformanceStats === "function",
    );

    await expect(page.locator(".performance-dragbar")).toHaveCount(1);
    await expect(page.locator(".performance-dragbar")).not.toBeVisible();
    const titleBefore = await page
        .getByRole("heading", { name: "StarCraft II" })
        .boundingBox();
    expect(titleBefore).not.toBeNull();

    await page.evaluate(() => {
        const runtime = window as typeof window & {
            updatePerformanceStats: (payload: {
                processTitle: string;
                sc2Ram: string;
                sc2Read: string;
                sc2ReadTotal: string;
                sc2Write: string;
                sc2WriteTotal: string;
                sc2Cpu: string;
                sc2CpuLevel: "low" | "normal" | "high";
                systemRam: string;
                systemRamLevel: "low" | "normal" | "high";
                systemDown: string;
                systemDownTotal: string;
                systemUp: string;
                systemUpTotal: string;
                cpuTotal: string;
                cpuTotalLevel: "low" | "normal" | "high";
                cpuCores: Array<{
                    label: string;
                    value: string;
                    level: "low" | "normal" | "high";
                }>;
            }) => void;
            setPerformanceEditMode: (enabled: boolean) => void;
        };

        runtime.setPerformanceEditMode(true);
        runtime.updatePerformanceStats({
            processTitle: "StarCraft II",
            sc2Ram: "12% | 842 MB",
            sc2Read: "1.1 MB/s",
            sc2ReadTotal: "379.5 GB",
            sc2Write: "149.3 kB/s",
            sc2WriteTotal: "6.3 GB",
            sc2Cpu: "95.0%",
            sc2CpuLevel: "high",
            systemRam: "104.3/127.9 GB",
            systemRamLevel: "normal",
            systemDown: "1.1 MB/s",
            systemDownTotal: "379.5 GB",
            systemUp: "149.3 kB/s",
            systemUpTotal: "6.3 GB",
            cpuTotal: "92.4%",
            cpuTotalLevel: "high",
            cpuCores: [
                { label: "CPU0", value: "95.0%", level: "high" },
                { label: "CPU1", value: "89.1%", level: "high" },
            ],
        });
    });

    await expect(page.locator(".performance-dragbar")).toBeVisible();
    await expect(page.getByText("Drag performance overlay")).toBeVisible();
    await expect(page.getByText("104.3/127.9 GB")).toBeVisible();
    await expect(page.getByText("95.0%")).toHaveCount(2);
    await expect(page.getByText("CPU1")).toBeVisible();
    const titleAfter = await page
        .getByRole("heading", { name: "StarCraft II" })
        .boundingBox();
    expect(titleAfter).not.toBeNull();
    if (titleBefore !== null && titleAfter !== null) {
        expect(Math.abs(titleAfter.y - titleBefore.y)).toBeLessThan(1);
    }

    const titleBoxes = await Promise.all([
        page.getByRole("heading", { name: "StarCraft II" }).boundingBox(),
        page.getByRole("heading", { name: "System" }).boundingBox(),
    ]);
    expect(titleBoxes[0]).not.toBeNull();
    expect(titleBoxes[1]).not.toBeNull();
    if (titleBoxes[0] !== null && titleBoxes[1] !== null) {
        expect(titleBoxes[1].x).toBeGreaterThan(titleBoxes[0].x + 120);
        expect(Math.abs(titleBoxes[1].y - titleBoxes[0].y)).toBeLessThan(16);
    }
});
