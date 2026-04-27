import * as React from "react";
import type {
    GamesRowPayload,
    LocalizedText,
    ReplayChatPayload,
    ReplayVisualPayload,
    UiMutatorRow,
} from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import styles from "../page.module.css";
import ReplayVisualPlayer from "./ReplayVisualPlayer";
import {
    nextSortState,
    sortIndicator,
    sortRows,
    type SortState,
} from "./tableSort";
import {
    clampPageNumber,
    TABLE_ROWS_PER_PAGE,
    pageCountForRows,
    rowsForPage,
    TablePagination,
} from "./tablePagination";
import type {
    DisplayValue,
    DifficultyFilterKey,
    DifficultyFilters,
    MutatorData,
} from "../types";

type GamesTabState = {
    isBusy: boolean;
    selectedReplayFile: string;
    setSelectedReplayFile: (file: string) => void;
    searchText: string;
    setSearchText: (value: string) => void;
    totalRows?: number;
    loadedRows?: number;
    ensureAllRowsLoaded?: () => Promise<void>;
    ensureRowsForPage?: (page: number, rowsPerPage: number) => Promise<void>;
    refresh: () => void;
    showReplay: (file: string) => void;
    loadChat: (file: string) => Promise<ReplayChatPayload | null>;
    loadVisual: (file: string) => Promise<ReplayVisualPayload | null>;
    revealFile: (file: string) => void;
};

type GamesTabProps = {
    rows: readonly GamesRowPayload[] | null;
    state: GamesTabState;
    asTableValue: (value: DisplayValue) => string;
    formatDurationSeconds: (value: DisplayValue) => string;
    languageManager: LanguageManager;
};

function asTableValueCompat(value: DisplayValue) {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

function formatDurationSecondsCompat(value: DisplayValue) {
    const seconds = Number(value);
    if (!Number.isFinite(seconds) || seconds <= 0 || seconds >= 999999) {
        return "-";
    }
    const total = Math.floor(seconds);
    const hh = Math.floor(total / 3600);
    const mm = Math.floor((total % 3600) / 60);
    const ss = total % 60;
    if (hh > 0) {
        return `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
    }
    return `${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
}

function mutatorIconPath(iconName: string): string {
    return `/overlay/Mutator Icons/${encodeURIComponent(iconName)}.png`;
}

function readMutators(
    value: readonly UiMutatorRow[] | null | undefined,
): readonly UiMutatorRow[] {
    if (!Array.isArray(value)) {
        return [];
    }
    return value;
}

function localizedMutatorName(
    mutator: MutatorData,
    languageManager: LanguageManager,
    asTableValue: (value: DisplayValue) => string,
): string {
    return asTableValue(
        languageManager.localizedValue(
            mutator.name as LocalizedText | null | undefined,
        ),
    );
}

function localizedMutatorDescription(
    mutator: MutatorData,
    languageManager: LanguageManager,
    asTableValue: (value: DisplayValue) => string,
): string {
    return asTableValue(
        languageManager.localizedValue(
            mutator.description as LocalizedText | null | undefined,
        ),
    );
}

function difficultyFilterKeyForRow(row: GamesRowPayload): DifficultyFilterKey {
    const brutalPlus = Number(row.brutal_plus ?? 0);
    if (Number.isFinite(brutalPlus) && brutalPlus > 0) {
        switch (brutalPlus) {
            case 1: {
                return "BrutalPlus1";
            }
            case 2: {
                return "BrutalPlus2";
            }
            case 3: {
                return "BrutalPlus3";
            }
            case 4: {
                return "BrutalPlus4";
            }
            case 5: {
                return "BrutalPlus5";
            }
            case 6: {
                return "BrutalPlus6";
            }
            default: {
                console.error(
                    `Brutal plus should be in range 1~6, but is ${brutalPlus}`,
                );
                return "Brutal";
            }
        }
    }

    const difficulty = String(row.difficulty || "")
        .trim()
        .toLowerCase();
    if (difficulty === "casual") {
        return "Casual";
    }
    if (difficulty === "normal") {
        return "Normal";
    }
    if (difficulty === "hard") {
        return "Hard";
    }
    return "Brutal";
}

function difficultyDisplayLabel(
    row: GamesRowPayload,
    languageManager: LanguageManager,
): string {
    const brutalPlus = Number(row.brutal_plus ?? 0);
    const suffixes: string[] = [];
    if (row.weekly === true) {
        suffixes.push(languageManager.translate("ui_overlay_weekly"));
    } else if (row.extension === true) {
        suffixes.push(languageManager.translate("ui_overlay_custom"));
    }
    const modeSuffix = suffixes.length > 0 ? ` (${suffixes.join(", ")})` : "";
    if (Number.isFinite(brutalPlus) && brutalPlus > 0) {
        return `${languageManager.localize(`B+${Math.min(6, brutalPlus)}`)}${modeSuffix}`;
    }
    return `${languageManager.localize(row.difficulty)}${modeSuffix}`;
}

export default function GamesTab({
    rows,
    state,
    asTableValue = asTableValueCompat,
    formatDurationSeconds = formatDurationSecondsCompat,
    languageManager,
}: GamesTabProps) {
    const t = (id: string) => languageManager.translate(id);
    const formatText = (
        id: string,
        values: Record<string, string | number> = {},
    ): string =>
        Object.entries(values).reduce(
            (text, [key, value]) =>
                text.split(`{{${key}}}`).join(String(value)),
            t(id),
        );
    const data: readonly GamesRowPayload[] = Array.isArray(rows) ? rows : [];
    const searchText = (state.searchText || "").trim().toLowerCase();
    const [sortState, setSortState] = React.useState<SortState>({
        key: "time",
        direction: "desc",
    });
    const [difficultyFilters, setDifficultyFilters] =
        React.useState<DifficultyFilters>({
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
        });
    const [includeNormalGames, setIncludeNormalGames] =
        React.useState<boolean>(true);
    const [includeMutationGames, setIncludeMutationGames] =
        React.useState<boolean>(true);
    const [currentPage, setCurrentPage] = React.useState<number>(1);
    const [chatModalOpen, setChatModalOpen] = React.useState<boolean>(false);
    const [chatLoading, setChatLoading] = React.useState<boolean>(false);
    const [chatError, setChatError] = React.useState<string>("");
    const [chatPayload, setChatPayload] =
        React.useState<ReplayChatPayload | null>(null);
    const chatRequestSeq = React.useRef<number>(0);
    const [visualModalOpen, setVisualModalOpen] =
        React.useState<boolean>(false);
    const [visualLoading, setVisualLoading] = React.useState<boolean>(false);
    const [visualError, setVisualError] = React.useState<string>("");
    const [visualPayload, setVisualPayload] =
        React.useState<ReplayVisualPayload | null>(null);
    const visualRequestSeq = React.useRef<number>(0);

    const formatReplayTime = (value: DisplayValue) => {
        const num = Number(value);
        if (!Number.isFinite(num) || num <= 0) {
            return "-";
        }
        const date = new Date(num * 1000);
        if (Number.isNaN(date.getTime())) {
            return "-";
        }
        const year = date.getFullYear();
        const month = String(date.getMonth() + 1).padStart(2, "0");
        const day = String(date.getDate()).padStart(2, "0");
        const hh = String(date.getHours()).padStart(2, "0");
        const mm = String(date.getMinutes()).padStart(2, "0");
        return `${year}-${month}-${day} ${hh}:${mm}`;
    };

    const formatChatTime = (value: DisplayValue) => {
        const seconds = Number(value);
        if (!Number.isFinite(seconds) || seconds < 0) {
            return "--:--";
        }
        const total = Math.floor(seconds);
        const hh = Math.floor(total / 3600);
        const mm = Math.floor((total % 3600) / 60);
        const ss = total % 60;
        if (hh > 0) {
            return `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
        }
        return `${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
    };

    const chatPlayerLabel = (
        payload: ReplayChatPayload,
        playerValue: DisplayValue,
    ) => {
        const player = Number(playerValue);
        if (player === 1) {
            return (
                asTableValue(payload.slot1_name).trim() ||
                t("ui_games_player_1_fallback")
            );
        }
        if (player === 2) {
            return (
                asTableValue(payload.slot2_name).trim() ||
                t("ui_games_player_2_fallback")
            );
        }
        return t("ui_games_system");
    };

    const closeChatModal = () => {
        setChatModalOpen(false);
        setChatLoading(false);
        setChatError("");
        setChatPayload(null);
    };

    const closeVisualModal = () => {
        setVisualModalOpen(false);
        setVisualLoading(false);
        setVisualError("");
        setVisualPayload(null);
    };

    const openChatModal = async (file: string) => {
        if (!file) {
            return;
        }
        state.setSelectedReplayFile(file);
        const requestSeq = chatRequestSeq.current + 1;
        chatRequestSeq.current = requestSeq;
        setChatModalOpen(true);
        setChatLoading(true);
        setChatError("");
        setChatPayload(null);

        try {
            const payload = await state.loadChat(file);
            if (chatRequestSeq.current !== requestSeq) {
                return;
            }
            if (payload === null) {
                setChatError(t("ui_games_chat_no_chat_available"));
                setChatPayload(null);
                return;
            }
            setChatPayload(payload);
        } catch (error) {
            if (chatRequestSeq.current !== requestSeq) {
                return;
            }
            const message =
                error instanceof Error
                    ? error.message
                    : t("ui_games_chat_failed");
            setChatError(message);
            setChatPayload(null);
        } finally {
            if (chatRequestSeq.current === requestSeq) {
                setChatLoading(false);
            }
        }
    };

    const openVisualModal = async (file: string) => {
        if (!file) {
            return;
        }
        state.setSelectedReplayFile(file);
        const requestSeq = visualRequestSeq.current + 1;
        visualRequestSeq.current = requestSeq;
        setVisualModalOpen(true);
        setVisualLoading(true);
        setVisualError("");
        setVisualPayload(null);

        try {
            const payload = await state.loadVisual(file);
            if (visualRequestSeq.current !== requestSeq) {
                return;
            }
            if (payload === null) {
                setVisualError(t("ui_games_visual_no_data"));
                setVisualPayload(null);
                return;
            }
            setVisualPayload(payload);
        } catch (error) {
            if (visualRequestSeq.current !== requestSeq) {
                return;
            }
            const message =
                error instanceof Error
                    ? error.message
                    : t("ui_games_visual_failed");
            setVisualError(message);
            setVisualPayload(null);
        } finally {
            if (visualRequestSeq.current === requestSeq) {
                setVisualLoading(false);
            }
        }
    };

    React.useEffect(() => {
        if (!chatModalOpen) {
            return undefined;
        }
        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                closeChatModal();
            }
        };
        window.addEventListener("keydown", handleKeyDown);
        return () => window.removeEventListener("keydown", handleKeyDown);
    }, [chatModalOpen]);

    React.useEffect(() => {
        if (!visualModalOpen) {
            return undefined;
        }
        const handleKeyDown = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                closeVisualModal();
            }
        };
        window.addEventListener("keydown", handleKeyDown);
        return () => window.removeEventListener("keydown", handleKeyDown);
    }, [visualModalOpen]);

    const filtered = React.useMemo(
        () =>
            data.filter((row) => {
                const difficultyKey = difficultyFilterKeyForRow(row);

                if (!difficultyFilters[difficultyKey]) {
                    return false;
                }

                const rowMutators = readMutators(row.mutators);
                const isMutation =
                    row.is_mutation === true || rowMutators.length > 0;
                if (!includeNormalGames && !isMutation) {
                    return false;
                }
                if (!includeMutationGames && isMutation) {
                    return false;
                }

                if (searchText === "") {
                    return true;
                }

                const mutatorSearch = rowMutators.flatMap((mutator) => [
                    mutator.id,
                    mutator.name,
                    localizedMutatorName(
                        mutator,
                        languageManager,
                        asTableValue,
                    ),
                    localizedMutatorDescription(
                        mutator,
                        languageManager,
                        asTableValue,
                    ),
                ]);
                const target = [
                    row.map,
                    languageManager.localize(row.map),
                    row.result,
                    languageManager.localize(row.result),
                    row.p1,
                    row.p2,
                    row.slot1_commander,
                    languageManager.localize(row.slot1_commander),
                    row.slot2_commander,
                    languageManager.localize(row.slot2_commander),
                    row.main_commander,
                    languageManager.localize(row.main_commander),
                    row.ally_commander,
                    languageManager.localize(row.ally_commander),
                    row.difficulty,
                    difficultyDisplayLabel(row, languageManager),
                    row.enemy,
                    languageManager.localize(row.enemy || "Unknown"),
                    row.file,
                    ...mutatorSearch,
                ]
                    .map((value) => asTableValue(value).toLowerCase())
                    .join(" ");
                return target.includes(searchText);
            }),
        [
            asTableValue,
            data,
            difficultyFilters,
            includeMutationGames,
            includeNormalGames,
            languageManager,
            searchText,
        ],
    );

    const sorted = React.useMemo(
        () =>
            sortRows(filtered, sortState, (row, key) => {
                if (key === "map") return languageManager.localize(row.map);
                if (key === "result")
                    return languageManager.localize(row.result);
                if (key === "p1")
                    return `${asTableValue(row.p1)} ${languageManager.localize(row.slot1_commander)}`;
                if (key === "p2")
                    return `${asTableValue(row.p2)} ${languageManager.localize(row.slot2_commander)}`;
                if (key === "enemy") {
                    return languageManager.localize(row.enemy || "Unknown");
                }
                if (key === "length") return Number(row.length || 0);
                if (key === "difficulty")
                    return difficultyDisplayLabel(row, languageManager);
                if (key === "mutators") {
                    return readMutators(row.mutators)
                        .map((mutator) =>
                            localizedMutatorName(
                                mutator,
                                languageManager,
                                asTableValue,
                            ),
                        )
                        .join(" ");
                }
                if (key === "time") return Number(row.date || 0);
                if (key === "actions") return row.file || "";
                return "";
            }),
        [asTableValue, filtered, languageManager, sortState],
    );
    const usingServerBackedPagination =
        searchText === "" &&
        includeNormalGames &&
        includeMutationGames &&
        Object.values(difficultyFilters).every(Boolean);
    const hasActiveClientFilters = !usingServerBackedPagination;
    const totalRowsForPagination = usingServerBackedPagination
        ? Math.max(Number(state.totalRows) || 0, sorted.length)
        : sorted.length;

    React.useEffect(() => {
        if (!hasActiveClientFilters) {
            return;
        }
        const loadedRows = Number(state.loadedRows) || 0;
        const totalRows = Number(state.totalRows) || 0;
        if (totalRows <= 0 || loadedRows >= totalRows) {
            return;
        }
        void state.ensureAllRowsLoaded?.();
    }, [
        hasActiveClientFilters,
        state.ensureAllRowsLoaded,
        state.loadedRows,
        state.totalRows,
    ]);

    React.useEffect(() => {
        setCurrentPage(1);
    }, [
        difficultyFilters,
        includeMutationGames,
        includeNormalGames,
        searchText,
    ]);

    React.useEffect(() => {
        setCurrentPage((page) =>
            clampPageNumber(page, pageCountForRows(totalRowsForPagination)),
        );
    }, [totalRowsForPagination]);

    const pagedRows = React.useMemo(
        () => rowsForPage(sorted, currentPage),
        [currentPage, sorted],
    );

    const handlePageChange = React.useCallback(
        (page: number) => {
            void (async () => {
                if (
                    usingServerBackedPagination &&
                    typeof state.ensureRowsForPage === "function"
                ) {
                    await state.ensureRowsForPage(page, TABLE_ROWS_PER_PAGE);
                }
                setCurrentPage(page);
            })();
        },
        [state, usingServerBackedPagination],
    );

    const columns = [
        { key: "map", label: t("ui_games_column_map") },
        { key: "result", label: t("ui_games_column_result") },
        { key: "p1", label: t("ui_games_column_player_1") },
        { key: "p2", label: t("ui_games_column_player_2") },
        { key: "enemy", label: t("ui_games_column_enemy") },
        { key: "length", label: t("ui_games_column_length") },
        { key: "difficulty", label: t("ui_games_column_difficulty") },
        { key: "mutators", label: t("ui_games_column_mutators") },
        { key: "time", label: t("ui_games_column_time") },
        { key: "actions", label: t("ui_games_column_actions") },
    ];

    return (
        <div className={styles.tabContent}>
            <section
                className={[styles.card, styles.group, styles.gamesPanel]
                    .filter(Boolean)
                    .join(" ")}
            >
                <div className={styles.gamesToolbar}>
                    <h3>{t("ui_tab_games")}</h3>
                    <div className={styles.gamesToolbarActions}>
                        <input
                            className={[styles.input, styles.gamesSearch]
                                .filter(Boolean)
                                .join(" ")}
                            type="text"
                            value={state.searchText || ""}
                            placeholder={t("ui_games_search")}
                            onChange={(event) =>
                                state.setSearchText(event.target.value)
                            }
                        />
                        <button
                            type="button"
                            className={[
                                styles.gamesIconBtn,
                                styles.buttonNormal,
                            ]
                                .filter(Boolean)
                                .join(" ")}
                            onClick={state.refresh}
                            disabled={state.isBusy}
                            title={t("ui_common_refresh")}
                        >
                            {state.isBusy ? "..." : "🔄"}
                        </button>
                    </div>
                </div>
                <div className={styles.gamesFilters}>
                    <div className={styles.gamesFilterGroup}>
                        <span className={styles.gamesFilterLabel}>
                            {t("ui_games_filter_difficulty")}
                        </span>
                        {(
                            [
                                ["Casual", "difficulty_casual"],
                                ["Normal", "difficulty_normal"],
                                ["Hard", "difficulty_hard"],
                                ["Brutal", "difficulty_brutal"],
                                ["BrutalPlus1", "difficulty_brutal_plus_1"],
                                ["BrutalPlus2", "difficulty_brutal_plus_2"],
                                ["BrutalPlus3", "difficulty_brutal_plus_3"],
                                ["BrutalPlus4", "difficulty_brutal_plus_4"],
                                ["BrutalPlus5", "difficulty_brutal_plus_5"],
                                ["BrutalPlus6", "difficulty_brutal_plus_6"],
                            ] as const
                        ).map(([key, labelId]) => (
                            <label
                                key={key}
                                className={styles.gamesFilterCheck}
                            >
                                <input
                                    type="checkbox"
                                    checked={difficultyFilters[key]}
                                    onChange={() =>
                                        setDifficultyFilters((current) => ({
                                            ...current,
                                            [key]: !current[key],
                                        }))
                                    }
                                />
                                <span>{t(labelId)}</span>
                            </label>
                        ))}
                    </div>
                    <div className={styles.gamesFilterGroup}>
                        <span className={styles.gamesFilterLabel}>
                            {t("ui_games_filter_mode")}
                        </span>
                        <label className={styles.gamesFilterCheck}>
                            <input
                                type="checkbox"
                                checked={includeNormalGames}
                                onChange={() =>
                                    setIncludeNormalGames((current) => !current)
                                }
                            />
                            <span>{t("ui_stats_normal_games")}</span>
                        </label>
                        <label className={styles.gamesFilterCheck}>
                            <input
                                type="checkbox"
                                checked={includeMutationGames}
                                onChange={() =>
                                    setIncludeMutationGames(
                                        (current) => !current,
                                    )
                                }
                            />
                            <span>{t("ui_stats_mutations")}</span>
                        </label>
                    </div>
                </div>
                <TablePagination
                    currentPage={currentPage}
                    onPageChange={handlePageChange}
                    totalRows={totalRowsForPagination}
                    hideWhenSinglePage={false}
                />
                <div className={styles.tableWrap} style={{ marginTop: "20px" }}>
                    <table
                        className={[styles.dataTable, styles.gamesTable]
                            .filter(Boolean)
                            .join(" ")}
                    >
                        <thead>
                            <tr>
                                {columns.map((column) => (
                                    <th key={`games-header-${column.key}`}>
                                        <button
                                            type="button"
                                            className={styles.tableSortBtn}
                                            onClick={() =>
                                                setSortState((current) =>
                                                    nextSortState(
                                                        current,
                                                        column.key,
                                                    ),
                                                )
                                            }
                                        >
                                            {`${column.label}${sortIndicator(sortState, column.key)}`}
                                        </button>
                                    </th>
                                ))}
                            </tr>
                        </thead>
                        <tbody>
                            {sorted.length === 0 ? (
                                <tr>
                                    <td
                                        colSpan={10}
                                        className={styles.emptyCell}
                                    >
                                        {data.length === 0
                                            ? t("ui_games_empty")
                                            : t("ui_games_empty_filtered")}
                                    </td>
                                </tr>
                            ) : (
                                pagedRows.map((row, idx) => {
                                    const p1 = asTableValue(row.p1);
                                    const p2 = asTableValue(row.p2);
                                    const p1Commander = asTableValue(
                                        languageManager.localize(
                                            row.slot1_commander,
                                        ),
                                    );
                                    const p2Commander = asTableValue(
                                        languageManager.localize(
                                            row.slot2_commander,
                                        ),
                                    );
                                    const p1Label = p1Commander
                                        ? `${p1} (${p1Commander})`
                                        : p1;
                                    const p2Label = p2Commander
                                        ? `${p2} (${p2Commander})`
                                        : p2;
                                    const file = row.file || "";
                                    const rowMutators = readMutators(
                                        row.mutators,
                                    );
                                    return (
                                        <tr
                                            key={`${file || "row"}-${idx}`}
                                            className={
                                                file ===
                                                state.selectedReplayFile
                                                    ? styles.selectedRow
                                                    : ""
                                            }
                                            onClick={() =>
                                                state.setSelectedReplayFile(
                                                    file,
                                                )
                                            }
                                        >
                                            <td>
                                                {languageManager.localize(
                                                    row.map,
                                                )}
                                            </td>
                                            <td>
                                                {languageManager.localize(
                                                    row.result,
                                                )}
                                            </td>
                                            <td
                                                className={
                                                    styles.gamesPlayerCell
                                                }
                                            >
                                                {p1Label}
                                            </td>
                                            <td
                                                className={
                                                    styles.gamesPlayerCell
                                                }
                                            >
                                                {p2Label}
                                            </td>
                                            <td>
                                                {asTableValue(
                                                    languageManager.localize(
                                                        row.enemy || "Unknown",
                                                    ),
                                                )}
                                            </td>
                                            <td>
                                                {formatDurationSeconds(
                                                    row.length,
                                                )}
                                            </td>
                                            <td>
                                                {difficultyDisplayLabel(
                                                    row,
                                                    languageManager,
                                                )}
                                            </td>
                                            <td>
                                                <div
                                                    className={
                                                        styles.gamesMutatorList
                                                    }
                                                >
                                                    {rowMutators.length ===
                                                    0 ? (
                                                        <span
                                                            className={
                                                                styles.gamesMutatorEmpty
                                                            }
                                                        >
                                                            -
                                                        </span>
                                                    ) : (
                                                        rowMutators.map(
                                                            (
                                                                mutator,
                                                                mutatorIndex,
                                                            ) => {
                                                                const iconName =
                                                                    asTableValue(
                                                                        mutator.iconName ||
                                                                            mutator
                                                                                .name
                                                                                ?.en ||
                                                                            mutator.id ||
                                                                            "",
                                                                    );
                                                                const displayName =
                                                                    localizedMutatorName(
                                                                        mutator,
                                                                        languageManager,
                                                                        asTableValue,
                                                                    );
                                                                const description =
                                                                    localizedMutatorDescription(
                                                                        mutator,
                                                                        languageManager,
                                                                        asTableValue,
                                                                    );
                                                                const tooltip =
                                                                    description ===
                                                                    ""
                                                                        ? displayName
                                                                        : `${displayName}\n${description}`;
                                                                return (
                                                                    <img
                                                                        key={`${asTableValue(mutator.id || mutator.name)}-${mutatorIndex}`}
                                                                        className={
                                                                            styles.gamesMutatorIcon
                                                                        }
                                                                        src={mutatorIconPath(
                                                                            iconName,
                                                                        )}
                                                                        alt={
                                                                            displayName
                                                                        }
                                                                        title={
                                                                            tooltip
                                                                        }
                                                                    />
                                                                );
                                                            },
                                                        )
                                                    )}
                                                </div>
                                            </td>
                                            <td>
                                                {formatReplayTime(row.date)}
                                            </td>
                                            <td
                                                className={
                                                    styles.gamesActionsCell
                                                }
                                            >
                                                <button
                                                    type="button"
                                                    className={[
                                                        styles.gamesRowBtn,
                                                        styles.buttonNormal,
                                                    ]
                                                        .filter(Boolean)
                                                        .join(" ")}
                                                    disabled={!file}
                                                    onClick={(event) => {
                                                        event.stopPropagation();
                                                        state.showReplay(file);
                                                    }}
                                                >
                                                    {t(
                                                        "ui_games_action_overlay",
                                                    )}
                                                </button>
                                                <button
                                                    type="button"
                                                    className={[
                                                        styles.gamesRowBtn,
                                                        styles.buttonNormal,
                                                    ]
                                                        .filter(Boolean)
                                                        .join(" ")}
                                                    disabled={!file}
                                                    onClick={(event) => {
                                                        event.stopPropagation();
                                                        void openVisualModal(
                                                            file,
                                                        );
                                                    }}
                                                >
                                                    {t(
                                                        "ui_games_action_visual",
                                                    )}
                                                </button>
                                                <button
                                                    type="button"
                                                    className={[
                                                        styles.gamesRowBtn,
                                                        styles.buttonNormal,
                                                    ]
                                                        .filter(Boolean)
                                                        .join(" ")}
                                                    disabled={!file}
                                                    onClick={(event) => {
                                                        event.stopPropagation();
                                                        void openChatModal(
                                                            file,
                                                        );
                                                    }}
                                                >
                                                    {t("ui_games_action_chat")}
                                                </button>
                                                <button
                                                    type="button"
                                                    className={[
                                                        styles.gamesRowBtn,
                                                        styles.buttonNormal,
                                                    ]
                                                        .filter(Boolean)
                                                        .join(" ")}
                                                    disabled={!file}
                                                    onClick={(event) => {
                                                        event.stopPropagation();
                                                        state.revealFile(file);
                                                    }}
                                                >
                                                    {t("ui_games_action_file")}
                                                </button>
                                            </td>
                                        </tr>
                                    );
                                })
                            )}
                        </tbody>
                    </table>
                </div>
                <TablePagination
                    currentPage={currentPage}
                    onPageChange={handlePageChange}
                    totalRows={totalRowsForPagination}
                    hideWhenSinglePage={false}
                />
            </section>
            {chatModalOpen ? (
                <div
                    className={styles.chatModalBackdrop}
                    onClick={closeChatModal}
                    role="presentation"
                >
                    <div
                        className={styles.chatModal}
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="chat-modal-title"
                        onClick={(event) => event.stopPropagation()}
                    >
                        <div className={styles.chatModalHeader}>
                            <div className={styles.chatModalHeading}>
                                <h3 id="chat-modal-title">
                                    {t("ui_games_chat_title")}
                                </h3>
                                <p className={styles.chatModalMeta}>
                                    {chatPayload
                                        ? `${asTableValue(chatPayload.map) || t("ui_games_unknown_map")} | ${asTableValue(chatPayload.result) || t("ui_games_unknown_result")} | ${formatReplayTime(chatPayload.date)}`
                                        : t("ui_games_chat_loading")}
                                </p>
                            </div>
                            <button
                                type="button"
                                className={[
                                    styles.gamesRowBtn,
                                    styles.chatModalClose,
                                    styles.buttonNormal,
                                ]
                                    .filter(Boolean)
                                    .join(" ")}
                                onClick={closeChatModal}
                            >
                                {t("ui_common_close")}
                            </button>
                        </div>
                        <div className={styles.chatModalBody}>
                            {chatLoading ? (
                                <p className={styles.chatEmpty}>
                                    {t("ui_common_loading")}
                                </p>
                            ) : chatError ? (
                                <p className={styles.chatEmpty}>{chatError}</p>
                            ) : !chatPayload ||
                              !Array.isArray(chatPayload.messages) ||
                              chatPayload.messages.length === 0 ? (
                                <p className={styles.chatEmpty}>
                                    {t("ui_games_chat_no_messages")}
                                </p>
                            ) : (
                                <div className={styles.chatLog}>
                                    {chatPayload.messages.map(
                                        (message, index) => (
                                            <div
                                                key={`chat-line-${index}-${asTableValue(message.time)}`}
                                                className={styles.chatRow}
                                            >
                                                <span
                                                    className={styles.chatTime}
                                                >
                                                    {formatChatTime(
                                                        message.time,
                                                    )}
                                                </span>
                                                <span
                                                    className={
                                                        styles.chatPlayer
                                                    }
                                                >
                                                    {chatPlayerLabel(
                                                        chatPayload,
                                                        message.player,
                                                    )}
                                                </span>
                                                <span
                                                    className={styles.chatText}
                                                >
                                                    {asTableValue(
                                                        message.text,
                                                    ) || "-"}
                                                </span>
                                            </div>
                                        ),
                                    )}
                                </div>
                            )}
                        </div>
                    </div>
                </div>
            ) : null}
            {visualModalOpen ? (
                <div
                    className={styles.chatModalBackdrop}
                    onClick={closeVisualModal}
                    role="presentation"
                >
                    <div
                        className={[styles.chatModal, styles.visualModal]
                            .filter(Boolean)
                            .join(" ")}
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="visual-modal-title"
                        onClick={(event) => event.stopPropagation()}
                    >
                        <div className={styles.chatModalHeader}>
                            <div className={styles.chatModalHeading}>
                                <h3 id="visual-modal-title">
                                    {t("ui_games_visual_title")}
                                </h3>
                                <p className={styles.chatModalMeta}>
                                    {visualPayload
                                        ? `${asTableValue(visualPayload.map) || t("ui_games_unknown_map")} | ${asTableValue(visualPayload.result) || t("ui_games_unknown_result")} | ${formatDurationSeconds(visualPayload.duration_seconds)}`
                                        : t("ui_games_visual_loading")}
                                </p>
                            </div>
                            <button
                                type="button"
                                className={[
                                    styles.gamesRowBtn,
                                    styles.chatModalClose,
                                    styles.buttonNormal,
                                ]
                                    .filter(Boolean)
                                    .join(" ")}
                                onClick={closeVisualModal}
                            >
                                {t("ui_common_close")}
                            </button>
                        </div>
                        <div className={styles.chatModalBody}>
                            {visualLoading ? (
                                <p className={styles.chatEmpty}>
                                    {t("ui_common_loading")}
                                </p>
                            ) : visualError ? (
                                <p className={styles.chatEmpty}>
                                    {visualError}
                                </p>
                            ) : !visualPayload ||
                              !Array.isArray(visualPayload.frames) ||
                              visualPayload.frames.length === 0 ? (
                                <p className={styles.chatEmpty}>
                                    {t("ui_games_visual_no_frames")}
                                </p>
                            ) : (
                                <ReplayVisualPlayer
                                    payload={visualPayload}
                                    t={t}
                                    asTableValue={asTableValue}
                                    localizeUnitName={(value) =>
                                        languageManager.localizeUnitName(value)
                                    }
                                    formatDurationSeconds={
                                        formatDurationSeconds
                                    }
                                />
                            )}
                        </div>
                    </div>
                </div>
            ) : null}
        </div>
    );
}
