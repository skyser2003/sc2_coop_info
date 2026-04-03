import { expect, test, type Page } from "@playwright/test";

async function installConfigTauriMock(page: Page) {
    await page.addInitScript(() => {
        type ConfigSettings = Record<string, unknown>;
        type ConfigRequest = {
            path: string;
            method: string;
            body?: {
                settings?: ConfigSettings;
                persist?: boolean;
            };
        };
        type ConfigMockWindow = Window & {
            __SCO_CONFIG_APPLY_REQUESTS__: ConfigSettings[];
            __SCO_CONFIG_SAVE_REQUESTS__: ConfigSettings[];
            __TAURI_INTERNALS__: {
                invoke: (
                    command: string,
                    request: ConfigRequest,
                ) => Promise<unknown>;
                event: {
                    listen: () => Promise<() => void>;
                };
            };
        };
        const runtimeWindow = window as ConfigMockWindow;
        const cloneJson = <T>(value: T): T => JSON.parse(JSON.stringify(value));
        let settings: Record<string, unknown> = {
            account_folder: "fixtures/accounts",
            main_names: [],
            detailed_analysis_atstart: false,
            rng_choices: {},
        };
        let activeSettings = cloneJson(settings);

        runtimeWindow.__SCO_CONFIG_APPLY_REQUESTS__ = [];
        runtimeWindow.__SCO_CONFIG_SAVE_REQUESTS__ = [];
        runtimeWindow.__TAURI_INTERNALS__ = {
            invoke: async (command: string, request: ConfigRequest) => {
                if (command !== "config_request") {
                    throw new Error(`Unexpected command: ${command}`);
                }

                if (request.method === "GET" && request.path === "/config") {
                    return {
                        status: "ok",
                        settings,
                        active_settings: activeSettings,
                        randomizer_catalog: {},
                        monitor_catalog: [
                            { index: 1, label: "1 - Primary Monitor" },
                        ],
                    };
                }

                if (request.method === "POST" && request.path === "/config") {
                    const nextSettings =
                        request.body?.settings || activeSettings;
                    activeSettings = cloneJson(nextSettings);
                    if (request.body?.persist === false) {
                        runtimeWindow.__SCO_CONFIG_APPLY_REQUESTS__.push(
                            activeSettings,
                        );
                    } else {
                        settings = cloneJson(nextSettings);
                        activeSettings = cloneJson(nextSettings);
                        runtimeWindow.__SCO_CONFIG_SAVE_REQUESTS__.push(
                            settings,
                        );
                    }
                    return {
                        status: "ok",
                        settings,
                        active_settings: activeSettings,
                        randomizer_catalog: {},
                        monitor_catalog: [
                            { index: 1, label: "1 - Primary Monitor" },
                        ],
                    };
                }

                if (request.method === "POST") {
                    return {
                        status: "ok",
                        result: { ok: true },
                        message: "ok",
                    };
                }

                return {
                    status: "ok",
                    replays: [],
                    players: [],
                    weeklies: [],
                    stats: {
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
                    },
                };
            },
            event: {
                listen: async () => () => {},
            },
        };
    });
}

test.describe.configure({ timeout: 60_000 });

test("show charts defaults to enabled and saves the replay overlay toggle", async ({
    page,
}) => {
    await installConfigTauriMock(page);
    await page.goto("/", { waitUntil: "domcontentloaded" });

    const showCharts = page.getByRole("checkbox", { name: "Show charts" });
    await expect(showCharts).toBeChecked();

    await showCharts.uncheck();

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
            show_charts: false,
        });

    await page.getByRole("button", { name: /^Save$/ }).click();

    await expect
        .poll(() =>
            page.evaluate(() => {
                const runtimeWindow = window as Window & {
                    __SCO_CONFIG_SAVE_REQUESTS__?: Array<
                        Record<string, unknown>
                    >;
                };
                const requests =
                    runtimeWindow.__SCO_CONFIG_SAVE_REQUESTS__ || [];
                return requests[requests.length - 1] || null;
            }),
        )
        .toMatchObject({
            show_charts: false,
        });
});
