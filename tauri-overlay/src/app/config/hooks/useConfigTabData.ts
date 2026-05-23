import * as React from "react";
import type {
    ConfigPlayersPayload,
    ConfigReplaysPayload,
    ConfigWeekliesPayload,
    GamesRowPayload,
    PlayerRowPayload,
    WeeklyRowPayload,
} from "../../../bindings/overlay";
import type { StatisticsPayload } from "../types";
import {
    loadPlayersRequest,
    loadReplaysRequest,
    loadWeekliesRequest,
} from "../configApi";
import type { GamesPageRequest } from "../tabs/GamesTab";
import type { PlayersPageRequest } from "../tabs/PlayersTab";

type GamesRows = readonly GamesRowPayload[];
type PlayerRows = readonly PlayerRowPayload[];
export type WeekliesRows = readonly WeeklyRowPayload[];

export type GamesPayload = {
    rows: GamesRows;
    totalRows: number;
};

export type PlayersPayload = {
    rows: PlayerRows;
    totalRows: number;
};

export type TabDataState = {
    games: GamesPayload | null;
    players: PlayersPayload | null;
    weeklies: WeekliesRows | null;
    statistics: StatisticsPayload | null;
};

export type LoadTabOptions = {
    gamesRequest?: GamesPageRequest;
    playersRequest?: PlayersPageRequest;
};

type UseConfigTabDataArgs = {
    setIsBusy: React.Dispatch<React.SetStateAction<boolean>>;
    safeStatus: (message: string) => void;
};

type UseConfigTabDataResult = {
    gamesPageRequestRef: React.MutableRefObject<GamesPageRequest>;
    loadTabData: (
        tabId: "games" | "players" | "weeklies",
        force?: boolean,
        options?: LoadTabOptions,
    ) => Promise<void>;
    playersPageRequestRef: React.MutableRefObject<PlayersPageRequest>;
    setTabData: React.Dispatch<React.SetStateAction<TabDataState>>;
    tabData: TabDataState;
};

function defaultGameDifficultyFilters(): GamesPageRequest["difficultyFilters"] {
    return {
        Casual: true,
        Normal: true,
        Hard: true,
        Brutal: true,
        BrutalPlus1: true,
        BrutalPlus2: true,
        BrutalPlus3: true,
        BrutalPlus4: true,
        BrutalPlus5: true,
        BrutalPlus6: true,
    };
}

function defaultGamesPageRequest(): GamesPageRequest {
    return {
        page: 1,
        rowsPerPage: 20,
        search: "",
        sortKey: "time",
        sortDirection: "desc",
        difficultyFilters: defaultGameDifficultyFilters(),
        includeNormalGames: true,
        includeMutationGames: true,
    };
}

function defaultPlayersPageRequest(): PlayersPageRequest {
    return {
        page: 1,
        rowsPerPage: 20,
        search: "",
        sortKey: "last_seen",
        sortDirection: "desc",
    };
}

function getTabPayload(
    tabId: "games" | "players" | "weeklies",
    payload:
        | ConfigReplaysPayload
        | ConfigPlayersPayload
        | ConfigWeekliesPayload,
): TabDataState["games" | "players" | "weeklies"] {
    if (tabId === "games") {
        const gamesPayload = payload as ConfigReplaysPayload;
        return {
            rows: gamesPayload.replays || [],
            totalRows:
                Number(gamesPayload.total_replays) ||
                (gamesPayload.replays || []).length,
        };
    }
    if (tabId === "players") {
        const playersPayload = payload as ConfigPlayersPayload;
        return {
            rows: playersPayload.players || [],
            totalRows:
                Number(playersPayload.total_players) ||
                (playersPayload.players || []).length,
        };
    }
    return (payload as ConfigWeekliesPayload).weeklies || [];
}

export function useConfigTabData({
    setIsBusy,
    safeStatus,
}: UseConfigTabDataArgs): UseConfigTabDataResult {
    const [tabData, setTabData] = React.useState<TabDataState>({
        games: null,
        players: null,
        weeklies: null,
        statistics: null,
    });
    const tabLoadInFlightRef = React.useRef<
        Record<"games" | "players" | "weeklies", boolean>
    >({
        games: false,
        players: false,
        weeklies: false,
    });
    const gamesPageRequestRef = React.useRef<GamesPageRequest>(
        defaultGamesPageRequest(),
    );
    const playersPageRequestRef = React.useRef<PlayersPageRequest>(
        defaultPlayersPageRequest(),
    );

    const loadTabData = React.useCallback(
        async (
            tabId: "games" | "players" | "weeklies",
            force = false,
            options: LoadTabOptions = {},
        ): Promise<void> => {
            if (!force && tabLoadInFlightRef.current[tabId]) {
                return;
            }
            tabLoadInFlightRef.current[tabId] = true;
            try {
                setIsBusy(true);
                const gamesRequest =
                    options.gamesRequest || gamesPageRequestRef.current;
                if (tabId === "games") {
                    gamesPageRequestRef.current = gamesRequest;
                }
                const playersRequest =
                    options.playersRequest || playersPageRequestRef.current;
                if (tabId === "players") {
                    playersPageRequestRef.current = playersRequest;
                }
                const payload =
                    tabId === "games"
                        ? await loadReplaysRequest(gamesRequest)
                        : tabId === "players"
                          ? await loadPlayersRequest(playersRequest)
                          : await loadWeekliesRequest();
                setTabData((current) => ({
                    ...current,
                    [tabId]: getTabPayload(tabId, payload),
                }));
                safeStatus(`${tabId} refreshed`);
            } catch (error) {
                safeStatus(`Failed to load ${tabId}: ${error.message}`);
            } finally {
                tabLoadInFlightRef.current[tabId] = false;
                setIsBusy(false);
            }
        },
        [safeStatus, setIsBusy],
    );

    return {
        gamesPageRequestRef,
        loadTabData,
        playersPageRequestRef,
        setTabData,
        tabData,
    };
}
