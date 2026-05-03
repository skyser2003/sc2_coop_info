import { expect, test, type Page } from "@playwright/test";

type WeeklyRow = {
    mutation: string;
    mutationOrder: number;
    isCurrent?: boolean;
    nextDuration?: string;
    nextDurationDays?: number;
    difficulty: string;
    wins: number;
    losses: number;
    winrate: number;
};

async function installWeekliesLanguageMock(
    page: Page,
    weeklies: readonly WeeklyRow[],
): Promise<void> {
    await page.addInitScript(
        ({ initialWeeklies }) => {
            const settings = {
                account_folder: "fixtures/accounts",
                language: "ko",
                main_names: [],
                detailed_analysis_atstart: false,
                rng_choices: {},
            };
            let activeSettings = JSON.parse(JSON.stringify(settings));
            const cloneJson = <T>(value: T): T =>
                JSON.parse(JSON.stringify(value)) as T;
            const configPayload = () => ({
                status: "ok",
                settings,
                active_settings: activeSettings,
                randomizer_catalog: {},
                monitor_catalog: [],
            });
            const weekliesPayload = () => ({
                status: "ok",
                weeklies: initialWeeklies,
            });
            const replaysPayload = () => ({
                status: "ok",
                replays: [],
                total_replays: 0,
                selected_replay_file: "",
            });
            const playersPayload = () => ({
                status: "ok",
                players: [],
                total_players: 0,
                loading: false,
            });

            window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener: () => {},
            };

            window.__TAURI_INTERNALS__ = {
                invoke: async (
                    command: string,
                    request?: {
                        body?: Record<string, unknown>;
                        method?: string;
                        path?: string;
                    },
                ) => {
                    if (command === "plugin:app|version") {
                        return "0.1.0";
                    }
                    if (command === "plugin:event|listen") {
                        return 1;
                    }
                    if (command === "plugin:event|unlisten") {
                        return null;
                    }
                    if (command === "is_dev") {
                        return true;
                    }
                    if (command === "config_get") {
                        return configPayload();
                    }
                    if (command === "config_update") {
                        if (request?.body?.settings) {
                            activeSettings = cloneJson(
                                request.body.settings,
                            ) as typeof activeSettings;
                        }
                        return configPayload();
                    }
                    if (command === "config_weeklies_get") {
                        return weekliesPayload();
                    }
                    if (command === "config_players_get") {
                        return playersPayload();
                    }
                    if (command === "config_replays_get") {
                        return replaysPayload();
                    }
                    if (command === "config_stats_get") {
                        return {
                            status: "ok",
                            ready: true,
                            games: 0,
                            analysis_running: false,
                            analysis_running_mode: null,
                            message: "",
                            query: "",
                            analysis: {
                                MapData: {},
                                CommanderData: {},
                                AllyCommanderData: {},
                                DifficultyData: {},
                                RegionData: {},
                                PlayerData: {},
                                AmonData: {},
                                MapDataReady: true,
                                UnitData: {
                                    main: {},
                                    ally: {},
                                    amon: {},
                                },
                            },
                        };
                    }
                    if (command === "config_action") {
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
                        };
                    }
                    if (command === "config_stats_action") {
                        return { status: "ok", message: "ok" };
                    }
                    if (command !== "config_request") {
                        throw new Error(`Unexpected command: ${command}`);
                    }

                    const method = request?.method;
                    const path = request?.path;

                    if (method === "GET" && path === "/config") {
                        return configPayload();
                    }

                    if (method === "GET" && path === "/config/weeklies") {
                        return weekliesPayload();
                    }

                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/players?")
                    ) {
                        return playersPayload();
                    }

                    if (
                        method === "GET" &&
                        path?.startsWith("/config/replays?")
                    ) {
                        return replaysPayload();
                    }

                    if (
                        method === "POST" &&
                        (path === "/config/action" ||
                            path === "/config/stats/action" ||
                            path === "/config")
                    ) {
                        if (path === "/config" && request?.body?.settings) {
                            activeSettings = JSON.parse(
                                JSON.stringify(request.body.settings),
                            );
                        }
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
                            settings,
                            active_settings: activeSettings,
                            randomizer_catalog: {},
                            monitor_catalog: [],
                        };
                    }

                    throw new Error(
                        `Unexpected request: ${String(method)} ${String(path)}`,
                    );
                },
                event: {
                    listen: async () => () => {},
                },
                transformCallback: (callback: () => void) => {
                    const id = Math.floor(Math.random() * 1000000);
                    window[`_${id}`] = callback;
                    return id;
                },
            };
        },
        {
            initialWeeklies: weeklies,
        },
    );
}

test("weeklies duration strings are localized in korean", async ({ page }) => {
    await installWeekliesLanguageMock(page, [
        {
            mutation: "Train of the Dead",
            mutationOrder: 0,
            isCurrent: true,
            nextDuration: "Now",
            nextDurationDays: 0,
            difficulty: "Brutal",
            wins: 5,
            losses: 0,
            winrate: 1,
        },
        {
            mutation: "First Strike",
            mutationOrder: 1,
            isCurrent: false,
            nextDuration: "2w 3d",
            nextDurationDays: 17,
            difficulty: "Brutal",
            wins: 4,
            losses: 2,
            winrate: 0.667,
        },
    ]);

    await page.goto("/#/config", { waitUntil: "domcontentloaded" });
    await page.getByRole("tab", { name: "주간 돌연변이" }).click();

    const rows = page.locator("tbody tr");
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0)).toContainText("현재");
    await expect(rows.nth(1)).toContainText("2주 3일");

    await expect(
        page.getByText("등장 시간: 현재", { exact: true }),
    ).toBeVisible();
});
