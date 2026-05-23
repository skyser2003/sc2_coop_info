import { invoke } from "@tauri-apps/api/core";
import type {
    AppSettings,
    ConfigChatPayload,
    ConfigPayload,
    ConfigPlayersPayload,
    ConfigReplayVisualPayload,
    ConfigReplaysPayload,
    ConfigWeekliesPayload,
    OverlayActionResponse,
    StatsActionPayload,
    StatsStatePayload,
} from "../../bindings/overlay";
import type { JsonObject } from "./types";
import type { GamesPageRequest } from "./tabs/GamesTab";
import type { PlayersPageRequest } from "./tabs/PlayersTab";

async function invokeConfigCommand<
    T extends { status?: string; message?: string },
>(command: string, args: JsonObject = {}): Promise<T> {
    const payload = await invoke<T>(command, args);
    if (!payload) {
        throw new Error(`Request failed (${command})`);
    }
    if (typeof payload.status === "string" && payload.status !== "ok") {
        throw new Error(payload?.message || `Request failed (${command})`);
    }
    return payload;
}

export function loadConfigRequest(): Promise<ConfigPayload> {
    return invokeConfigCommand<ConfigPayload>("config_get");
}

export function updateConfigRequest(
    settings: AppSettings,
    persist: boolean,
): Promise<ConfigPayload> {
    return invokeConfigCommand<ConfigPayload>("config_update", {
        settings,
        persist,
    });
}

function enabledDifficultyFilters(
    filters: GamesPageRequest["difficultyFilters"],
): string[] {
    return Object.entries(filters)
        .filter(([, enabled]) => enabled)
        .map(([key]) => key);
}

export function loadReplaysRequest(
    request: GamesPageRequest,
): Promise<ConfigReplaysPayload> {
    return invokeConfigCommand<ConfigReplaysPayload>("config_replays_get", {
        request: {
            page: request.page,
            rowsPerPage: request.rowsPerPage,
            search: request.search,
            sortKey: request.sortKey,
            sortDirection: request.sortDirection,
            difficultyFilters: enabledDifficultyFilters(
                request.difficultyFilters,
            ),
            includeNormalGames: request.includeNormalGames,
            includeMutationGames: request.includeMutationGames,
        },
    });
}

export function loadPlayersRequest(
    request: PlayersPageRequest,
): Promise<ConfigPlayersPayload> {
    return invokeConfigCommand<ConfigPlayersPayload>("config_players_get", {
        request: {
            page: request.page,
            rowsPerPage: request.rowsPerPage,
            search: request.search,
            sortKey: request.sortKey,
            sortDirection: request.sortDirection,
        },
    });
}

export function loadWeekliesRequest(): Promise<ConfigWeekliesPayload> {
    return invokeConfigCommand<ConfigWeekliesPayload>("config_weeklies_get");
}

export function loadStatisticsRequest(
    query: string,
): Promise<StatsStatePayload> {
    return invokeConfigCommand<StatsStatePayload>("config_stats_get", {
        query,
    });
}

export function postConfigActionRequest(
    action: string,
    payload: JsonObject = {},
): Promise<OverlayActionResponse> {
    return invokeConfigCommand<OverlayActionResponse>("config_action", {
        action,
        payload,
    });
}

export function postStatsActionRequest(
    action: string,
    payload: JsonObject = {},
): Promise<StatsActionPayload> {
    return invokeConfigCommand<StatsActionPayload>("config_stats_action", {
        action,
        payload,
    });
}

export function showReplayRequest(
    file: string | null,
): Promise<OverlayActionResponse> {
    return invokeConfigCommand<OverlayActionResponse>("config_replay_show", {
        file,
    });
}

export function loadReplayChatRequest(
    file: string,
): Promise<ConfigChatPayload> {
    return invokeConfigCommand<ConfigChatPayload>("config_replay_chat", {
        file,
    });
}

export function loadReplayVisualRequest(
    file: string,
): Promise<ConfigReplayVisualPayload> {
    return invokeConfigCommand<ConfigReplayVisualPayload>(
        "config_replay_visual",
        {
            file,
        },
    );
}

export function moveReplayRequest(
    delta: number,
): Promise<OverlayActionResponse> {
    return invokeConfigCommand<OverlayActionResponse>("config_replay_move", {
        delta,
    });
}
