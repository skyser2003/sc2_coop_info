import type { Page } from "@playwright/test";

export type ConfigMockMutator = {
    name: {
        en: string;
        ko: string;
    };
    iconName: string;
    description: {
        en: string;
        ko: string;
    };
};

export type ConfigMockGameRow = {
    map: string;
    result: string;
    p1: string;
    p2: string;
    main_commander: string;
    ally_commander: string;
    difficulty: string;
    file: string;
    length: number;
    date: number;
    enemy?: string;
    enemy_race?: string;
    brutal_plus?: number;
    weekly?: boolean;
    is_mutation?: boolean;
    mutators?: readonly ConfigMockMutator[];
};

export type ConfigMockPlayerRow = {
    player: string;
    wins: number;
    losses: number;
    winrate: number;
    apm: number;
    commander: string;
    kills: number;
    last_seen: number;
};

type ConfigMockSettingsValue =
    | string
    | number
    | boolean
    | null
    | readonly string[]
    | Record<string, boolean>;

export type ConfigMockSettings = Record<string, ConfigMockSettingsValue>;

export type ConfigMockPageRequest = {
    page?: number;
    rowsPerPage?: number;
    search?: string;
    sortKey?: string;
    sortDirection?: string;
    difficultyFilters?: readonly string[];
    includeNormalGames?: boolean;
    includeMutationGames?: boolean;
};

export type ConfigMockOptions = {
    games?: readonly ConfigMockGameRow[];
    players?: readonly ConfigMockPlayerRow[];
    weeklies?: readonly ConfigMockSettings[];
    settings?: ConfigMockSettings;
};

type ConfigMockInvokeRequest = {
    action?: string;
    body?: {
        action?: string;
        persist?: boolean;
        settings?: ConfigMockSettings;
    };
    command?: string;
    method?: string;
    path?: string;
    persist?: boolean;
    query?: string;
    request?: ConfigMockPageRequest;
    settings?: ConfigMockSettings;
};

type ConfigMockInitOptions = {
    games: readonly ConfigMockGameRow[];
    players: readonly ConfigMockPlayerRow[];
    weeklies: readonly ConfigMockSettings[];
    settings: ConfigMockSettings;
};

function defaultOptions(options: ConfigMockOptions): ConfigMockInitOptions {
    return {
        games: options.games ?? [],
        players: options.players ?? [],
        weeklies: options.weeklies ?? [],
        settings: {
            account_folder: "fixtures/accounts",
            main_names: [],
            detailed_analysis_atstart: false,
            rng_choices: {},
            ...(options.settings ?? {}),
        },
    };
}

export async function installConfigMock(
    page: Page,
    options: ConfigMockOptions = {},
): Promise<void> {
    await page.addInitScript(
        ({ games, players, weeklies, settings: initialSettings }) => {
            type BrowserGameRow = ConfigMockGameRow;
            type BrowserPlayerRow = ConfigMockPlayerRow;
            type BrowserPageRequest = ConfigMockPageRequest;
            type BrowserInvokeRequest = ConfigMockInvokeRequest;

            const cloneJson = <T>(value: T): T =>
                JSON.parse(JSON.stringify(value)) as T;
            let settings = cloneJson(initialSettings);
            let activeSettings = cloneJson(settings);

            const pageSlice = <T>(
                rows: readonly T[],
                pageRequest?: BrowserPageRequest,
            ): readonly T[] => {
                const page = Math.max(1, Number(pageRequest?.page) || 1);
                const rowsPerPage = Math.max(
                    1,
                    Number(pageRequest?.rowsPerPage) || 20,
                );
                const start = (page - 1) * rowsPerPage;
                return rows.slice(start, start + rowsPerPage);
            };

            const difficultyKey = (row: BrowserGameRow): string => {
                const brutalPlus = Number(row.brutal_plus || 0);
                if (brutalPlus > 0) return `BrutalPlus${brutalPlus}`;
                const difficulty = String(row.difficulty || "").toLowerCase();
                if (difficulty === "casual") return "Casual";
                if (difficulty === "normal") return "Normal";
                if (difficulty === "hard") return "Hard";
                return "Brutal";
            };

            const defaultDifficulties = [
                "Casual",
                "Normal",
                "Hard",
                "Brutal",
                "BrutalPlus1",
                "BrutalPlus2",
                "BrutalPlus3",
                "BrutalPlus4",
                "BrutalPlus5",
                "BrutalPlus6",
            ];

            const sortedGameRows = (
                rows: readonly BrowserGameRow[],
                pageRequest?: BrowserPageRequest,
            ): BrowserGameRow[] => {
                const sortKey = String(pageRequest?.sortKey || "time");
                const direction = pageRequest?.sortDirection === "asc" ? 1 : -1;
                const valueKey = sortKey === "time" ? "date" : sortKey;
                return [...rows].sort((left, right) => {
                    const leftValue = Number(left[valueKey]);
                    const rightValue = Number(right[valueKey]);
                    if (
                        Number.isFinite(leftValue) &&
                        Number.isFinite(rightValue) &&
                        leftValue !== rightValue
                    ) {
                        return leftValue < rightValue
                            ? -1 * direction
                            : direction;
                    }
                    return (
                        String(left[valueKey] || "").localeCompare(
                            String(right[valueKey] || ""),
                        ) * direction
                    );
                });
            };

            const sortedPlayerRows = (
                rows: readonly BrowserPlayerRow[],
                pageRequest?: BrowserPageRequest,
            ): BrowserPlayerRow[] => {
                const sortKey = String(pageRequest?.sortKey || "last_seen");
                const direction = pageRequest?.sortDirection === "asc" ? 1 : -1;
                return [...rows].sort((left, right) => {
                    const leftValue = Number(left[sortKey]);
                    const rightValue = Number(right[sortKey]);
                    if (
                        Number.isFinite(leftValue) &&
                        Number.isFinite(rightValue) &&
                        leftValue !== rightValue
                    ) {
                        return leftValue < rightValue
                            ? -1 * direction
                            : direction;
                    }
                    return (
                        String(left[sortKey] || "").localeCompare(
                            String(right[sortKey] || ""),
                        ) * direction
                    );
                });
            };

            const replaysPayload = (pageRequest?: BrowserPageRequest) => {
                const enabledDifficulties = new Set(
                    pageRequest?.difficultyFilters || defaultDifficulties,
                );
                const includeNormalGames =
                    pageRequest?.includeNormalGames !== false;
                const includeMutationGames =
                    pageRequest?.includeMutationGames !== false;
                const filteredRows = games.filter((row) => {
                    const isMutation =
                        row.is_mutation === true ||
                        row.weekly === true ||
                        Number(row.mutators?.length || 0) > 0;
                    if (!includeNormalGames && !isMutation) return false;
                    if (!includeMutationGames && isMutation) return false;
                    return enabledDifficulties.has(difficultyKey(row));
                });
                const sortedRows = sortedGameRows(filteredRows, pageRequest);
                return {
                    status: "ok",
                    replays: pageSlice(sortedRows, pageRequest),
                    total_replays: sortedRows.length,
                    selected_replay_file: "",
                };
            };

            const playersPayload = (pageRequest?: BrowserPageRequest) => {
                const sortedRows = sortedPlayerRows(players, pageRequest);
                return {
                    status: "ok",
                    players: pageSlice(sortedRows, pageRequest),
                    total_players: sortedRows.length,
                    loading: false,
                };
            };

            const configPayload = () => ({
                status: "ok",
                settings,
                active_settings: activeSettings,
                randomizer_catalog: {},
                monitor_catalog: [],
            });

            const statsPayload = () => ({
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
            });

            window.__SCO_ACTION_REQUESTS__ = [];
            window.__SCO_CONFIG_APPLY_REQUESTS__ = [];
            window.__SCO_CONFIG_SAVE_REQUESTS__ = [];
            window.__SCO_FOLDER_PICKER_REQUESTS__ = [];
            window.__SCO_TAB_REQUESTS__ = [];
            window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
                unregisterListener: () => {},
            };

            window.__TAURI_INTERNALS__ = {
                invoke: async (
                    command: string,
                    request?: BrowserInvokeRequest,
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
                    if (command === "plugin:event|emit") {
                        return null;
                    }
                    if (command === "is_dev") {
                        return true;
                    }
                    if (command === "config_get") {
                        return configPayload();
                    }
                    if (command === "config_update") {
                        const nextSettings = request?.settings || settings;
                        activeSettings = cloneJson(nextSettings);
                        if (request?.persist === false) {
                            window.__SCO_CONFIG_APPLY_REQUESTS__.push(
                                activeSettings,
                            );
                        } else {
                            settings = cloneJson(nextSettings);
                            activeSettings = cloneJson(nextSettings);
                            window.__SCO_CONFIG_SAVE_REQUESTS__.push(settings);
                        }
                        return configPayload();
                    }
                    if (command === "config_replays_get") {
                        window.__SCO_TAB_REQUESTS__.push({
                            command,
                            request: cloneJson(request || {}),
                        });
                        return replaysPayload(request?.request);
                    }
                    if (command === "config_players_get") {
                        window.__SCO_TAB_REQUESTS__.push({
                            command,
                            request: cloneJson(request || {}),
                        });
                        return playersPayload(request?.request);
                    }
                    if (command === "config_weeklies_get") {
                        window.__SCO_TAB_REQUESTS__.push({
                            command,
                            request: cloneJson(request || {}),
                        });
                        return {
                            status: "ok",
                            weeklies,
                        };
                    }
                    if (command === "config_stats_get") {
                        return statsPayload();
                    }
                    if (command === "config_action") {
                        window.__SCO_ACTION_REQUESTS__.push(request || null);
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
                        };
                    }
                    if (command === "config_stats_action") {
                        return { status: "ok", message: "ok" };
                    }
                    if (command === "config_replay_show") {
                        return { status: "ok", message: "Replay sent" };
                    }
                    if (command !== "config_request") {
                        throw new Error(`Unexpected command: ${command}`);
                    }

                    const method = request?.method;
                    const path = request?.path;

                    if (method === "GET" && path === "/config") {
                        return configPayload();
                    }
                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/replays?")
                    ) {
                        return replaysPayload();
                    }
                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/players?")
                    ) {
                        return playersPayload();
                    }
                    if (method === "GET" && path === "/config/weeklies") {
                        return {
                            status: "ok",
                            weeklies,
                        };
                    }
                    if (
                        method === "GET" &&
                        typeof path === "string" &&
                        path.startsWith("/config/stats?")
                    ) {
                        return { status: "ok", stats: statsPayload() };
                    }
                    if (method === "POST" && path === "/config/replays/show") {
                        return { status: "ok", message: "Replay sent" };
                    }
                    if (
                        method === "POST" &&
                        (path === "/config" ||
                            path === "/config/action" ||
                            path === "/config/stats/action")
                    ) {
                        return {
                            status: "ok",
                            result: { ok: true },
                            message: "ok",
                            settings,
                            active_settings: settings,
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
        defaultOptions(options),
    );
}
