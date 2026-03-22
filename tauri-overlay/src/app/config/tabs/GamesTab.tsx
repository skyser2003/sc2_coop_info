import * as React from "react";
import type { LanguageManager } from "../../i18n/languageManager";
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

type GamesTabRow = {
    map?: string | null;
    result?: string | null;
    p1?: string | null;
    p2?: string | null;
    main_commander?: string | null;
    ally_commander?: string | null;
    difficulty?: string | null;
    enemy?: string | null;
    enemy_race?: string | null;
    file?: string | null;
    length?: number | string | null;
    date?: number | string | null;
    brutal_plus?: number | string | null;
    extension?: boolean | null;
    weekly?: boolean | null;
    is_mutation?: boolean | null;
    mutators?: readonly GamesTabMutator[] | null;
};

type GamesTabMutator = {
    name?: string | null;
    nameEn?: string | null;
    nameKo?: string | null;
    iconName?: string | null;
    descriptionEn?: string | null;
    descriptionKo?: string | null;
};

type GamesTabChatMessage = {
    player?: number | null;
    text?: string | null;
    time?: number | null;
};

type GamesTabChatPayload = {
    file?: string | null;
    date?: number | string | null;
    map?: string | null;
    result?: string | null;
    slot1_name?: string | null;
    slot2_name?: string | null;
    messages?: readonly GamesTabChatMessage[] | null;
};

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
    loadChat: (file: string) => Promise<GamesTabChatPayload | null>;
    revealFile: (file: string) => void;
};

type GamesTabProps = {
    rows: readonly GamesTabRow[] | null;
    state: GamesTabState;
    asTableValue: (value: unknown) => string;
    formatDurationSeconds: (value: unknown) => string;
    languageManager: LanguageManager;
};

type DifficultyFilterKey =
    | "Casual"
    | "Normal"
    | "Hard"
    | "Brutal"
    | "BrutalPlus";

type DifficultyFilters = Record<DifficultyFilterKey, boolean>;

function asTableValueCompat(value: unknown) {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

function formatDurationSecondsCompat(value: unknown) {
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

function isGamesTabMutator(value: unknown): value is GamesTabMutator {
    return value !== null && typeof value === "object" && !Array.isArray(value);
}

function readMutators(value: unknown): readonly GamesTabMutator[] {
    if (!Array.isArray(value)) {
        return [];
    }
    return value.filter(isGamesTabMutator);
}

function localizedMutatorName(
    mutator: GamesTabMutator,
    languageManager: LanguageManager,
    asTableValue: (value: unknown) => string,
): string {
    const preferred =
        languageManager.currentLanguage() === "ko"
            ? mutator.nameKo
            : mutator.nameEn;
    const fallback =
        languageManager.currentLanguage() === "ko"
            ? mutator.nameEn
            : mutator.nameKo;
    return asTableValue(preferred || fallback || mutator.name);
}

function localizedMutatorDescription(
    mutator: GamesTabMutator,
    languageManager: LanguageManager,
    asTableValue: (value: unknown) => string,
): string {
    const preferred =
        languageManager.currentLanguage() === "ko"
            ? mutator.descriptionKo
            : mutator.descriptionEn;
    const fallback =
        languageManager.currentLanguage() === "ko"
            ? mutator.descriptionEn
            : mutator.descriptionKo;
    return asTableValue(preferred || fallback);
}

function difficultyFilterKeyForRow(row: GamesTabRow): DifficultyFilterKey {
    const brutalPlus = Number(row.brutal_plus ?? 0);
    if (Number.isFinite(brutalPlus) && brutalPlus > 0) {
        return "BrutalPlus";
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
    row: GamesTabRow,
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
    const data: readonly GamesTabRow[] = Array.isArray(rows) ? rows : [];
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
            BrutalPlus: true,
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
        React.useState<GamesTabChatPayload | null>(null);
    const chatRequestSeq = React.useRef<number>(0);

    const formatReplayTime = (value: unknown) => {
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

    const formatChatTime = (value: unknown) => {
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
        payload: GamesTabChatPayload,
        playerValue: unknown,
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
                    row.main_commander,
                    languageManager.localize(row.main_commander),
                    row.ally_commander,
                    languageManager.localize(row.ally_commander),
                    row.difficulty,
                    difficultyDisplayLabel(row, languageManager),
                    row.enemy,
                    languageManager.localize(
                        row.enemy || row.enemy_race || "Unknown",
                    ),
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
                    return `${asTableValue(row.p1)} ${languageManager.localize(row.main_commander)}`;
                if (key === "p2")
                    return `${asTableValue(row.p2)} ${languageManager.localize(row.ally_commander)}`;
                if (key === "enemy") {
                    return languageManager.localize(
                        row.enemy || row.enemy_race || "Unknown",
                    );
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
        <div className="tab-content">
            <section className="card group games-panel">
                <div className="games-toolbar">
                    <h3>{t("ui_tab_games")}</h3>
                    <div className="games-toolbar-actions">
                        <input
                            className="input games-search"
                            type="text"
                            value={state.searchText || ""}
                            placeholder={t("ui_games_search")}
                            onChange={(event) =>
                                state.setSearchText(event.target.value)
                            }
                        />
                        <button
                            type="button"
                            className="games-icon-btn"
                            onClick={state.refresh}
                            disabled={state.isBusy}
                            title={t("ui_common_refresh")}
                        >
                            {state.isBusy ? "..." : "🔄"}
                        </button>
                    </div>
                </div>
                <div className="games-filters">
                    <div className="games-filter-group">
                        <span className="games-filter-label">
                            {t("ui_games_filter_difficulty")}
                        </span>
                        {(
                            [
                                ["Casual", "difficulty_casual"],
                                ["Normal", "difficulty_normal"],
                                ["Hard", "difficulty_hard"],
                                ["Brutal", "difficulty_brutal"],
                                ["BrutalPlus", "difficulty_brutal_plus"],
                            ] as const
                        ).map(([key, labelId]) => (
                            <label key={key} className="games-filter-check">
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
                    <div className="games-filter-group">
                        <span className="games-filter-label">
                            {t("ui_games_filter_mode")}
                        </span>
                        <label className="games-filter-check">
                            <input
                                type="checkbox"
                                checked={includeNormalGames}
                                onChange={() =>
                                    setIncludeNormalGames((current) => !current)
                                }
                            />
                            <span>{t("ui_stats_normal_games")}</span>
                        </label>
                        <label className="games-filter-check">
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
                <div className="table-wrap">
                    <table className="data-table games-table">
                        <thead>
                            <tr>
                                {columns.map((column) => (
                                    <th key={`games-header-${column.key}`}>
                                        <button
                                            type="button"
                                            className="table-sort-btn"
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
                                    <td colSpan={10} className="empty-cell">
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
                                            row.main_commander,
                                        ),
                                    );
                                    const p2Commander = asTableValue(
                                        languageManager.localize(
                                            row.ally_commander,
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
                                                    ? "selected-row"
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
                                            <td className="games-player-cell">
                                                {p1Label}
                                            </td>
                                            <td className="games-player-cell">
                                                {p2Label}
                                            </td>
                                            <td>
                                                {asTableValue(
                                                    languageManager.localize(
                                                        row.enemy ||
                                                            row.enemy_race ||
                                                            "Unknown",
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
                                                <div className="games-mutator-list">
                                                    {rowMutators.length ===
                                                    0 ? (
                                                        <span className="games-mutator-empty">
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
                                                                            mutator.name,
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
                                                                        key={`${asTableValue(mutator.name)}-${mutatorIndex}`}
                                                                        className="games-mutator-icon"
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
                                            <td className="games-actions-cell">
                                                <button
                                                    type="button"
                                                    className="games-row-btn"
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
                                                    className="games-row-btn"
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
                                                    className="games-row-btn"
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
                    className="chat-modal-backdrop"
                    onClick={closeChatModal}
                    role="presentation"
                >
                    <div
                        className="chat-modal"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="chat-modal-title"
                        onClick={(event) => event.stopPropagation()}
                    >
                        <div className="chat-modal-header">
                            <div className="chat-modal-heading">
                                <h3 id="chat-modal-title">
                                    {t("ui_games_chat_title")}
                                </h3>
                                <p className="chat-modal-meta">
                                    {chatPayload
                                        ? `${asTableValue(chatPayload.map) || t("ui_games_unknown_map")} | ${asTableValue(chatPayload.result) || t("ui_games_unknown_result")} | ${formatReplayTime(chatPayload.date)}`
                                        : t("ui_games_chat_loading")}
                                </p>
                            </div>
                            <button
                                type="button"
                                className="games-row-btn chat-modal-close"
                                onClick={closeChatModal}
                            >
                                {t("ui_common_close")}
                            </button>
                        </div>
                        <div className="chat-modal-body">
                            {chatLoading ? (
                                <p className="chat-empty">
                                    {t("ui_common_loading")}
                                </p>
                            ) : chatError ? (
                                <p className="chat-empty">{chatError}</p>
                            ) : !chatPayload ||
                              !Array.isArray(chatPayload.messages) ||
                              chatPayload.messages.length === 0 ? (
                                <p className="chat-empty">
                                    {t("ui_games_chat_no_messages")}
                                </p>
                            ) : (
                                <div className="chat-log">
                                    {chatPayload.messages.map(
                                        (message, index) => (
                                            <div
                                                key={`chat-line-${index}-${asTableValue(message.time)}`}
                                                className="chat-row"
                                            >
                                                <span className="chat-time">
                                                    {formatChatTime(
                                                        message.time,
                                                    )}
                                                </span>
                                                <span className="chat-player">
                                                    {chatPlayerLabel(
                                                        chatPayload,
                                                        message.player,
                                                    )}
                                                </span>
                                                <span className="chat-text">
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
        </div>
    );
}
