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
    pageCountForRows,
    rowsForPage,
    TablePagination,
} from "./tablePagination";

type PlayersTabRow = {
    handle?: string | null;
    player?: string | null;
    player_names?: readonly string[] | null;
    wins?: number | string | null;
    losses?: number | string | null;
    winrate?: number | string | null;
    apm?: number | string | null;
    commander?: string | null;
    frequency?: number | string | null;
    kills?: number | string | null;
    last_seen?: number | string | null;
};

type PlayerNotes = Readonly<Record<string, string>>;

type PlayersTabProps = {
    rows: readonly PlayersTabRow[] | null;
    onRefresh: () => void;
    noteValues: PlayerNotes;
    onNoteChange: (handle: string, note: string) => void;
    onNoteCommit: (handle: string, note: string) => Promise<void>;
    isBusy: boolean;
    asTableValue?: (value: unknown) => string;
    formatPercent?: (value: unknown) => string;
    languageManager: LanguageManager;
};

type PlayersTableRow = PlayersTabRow & {
    readonly resolvedNote: string;
    readonly handleKey: string;
    readonly playerNamesList: readonly string[];
};

function asTableValueCompat(value: unknown) {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

function formatPercentCompat(value: unknown) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "0.0%";
    }
    return `${(num * 100).toFixed(1)}%`;
}

function formatReplayTime(value: unknown) {
    const num = Number(value);
    if (!Number.isFinite(num) || num <= 0) {
        return "-";
    }

    const date = new Date(num * 1000);
    if (Number.isNaN(date.getTime())) {
        return "-";
    }

    const year = date.getUTCFullYear();
    const month = String(date.getUTCMonth() + 1).padStart(2, "0");
    const day = String(date.getUTCDate()).padStart(2, "0");
    const hh = String(date.getUTCHours()).padStart(2, "0");
    const mm = String(date.getUTCMinutes()).padStart(2, "0");
    const ss = String(date.getUTCSeconds()).padStart(2, "0");
    return `${year}-${month}-${day} ${hh}:${mm}:${ss}`;
}

function normalizeHandleKey(value: string): string {
    return value.trim().toLowerCase();
}

function noteForHandle(handle: string, noteValues: PlayerNotes): string {
    const direct = noteValues[handle];
    if (typeof direct === "string") {
        return direct;
    }

    const normalizedHandle = normalizeHandleKey(handle);
    if (normalizedHandle === "") {
        return "";
    }

    for (const [key, value] of Object.entries(noteValues)) {
        if (normalizeHandleKey(key) === normalizedHandle) {
            return value;
        }
    }

    return "";
}

function uniquePlayerNames(
    value: readonly string[] | null | undefined,
): readonly string[] {
    if (!Array.isArray(value)) {
        return [];
    }

    const deduped = new Set<string>();
    for (const handle of value) {
        const normalized = asTableValueCompat(handle).trim();
        if (normalized !== "") {
            deduped.add(normalized);
        }
    }
    return Array.from(deduped);
}

export default function PlayersTab({
    rows,
    onRefresh,
    noteValues,
    onNoteChange,
    onNoteCommit,
    isBusy,
    asTableValue = asTableValueCompat,
    formatPercent = formatPercentCompat,
    languageManager,
}: PlayersTabProps) {
    const t = (id: string) => languageManager.translate(id);
    const data: readonly PlayersTableRow[] = React.useMemo(
        () =>
            (Array.isArray(rows) ? rows : []).map((row) => {
                const playerName = asTableValueCompat(row.player);
                const handle = asTableValueCompat(row.handle);
                return {
                    ...row,
                    resolvedNote: noteForHandle(handle, noteValues),
                    handleKey: normalizeHandleKey(handle),
                    player: playerName,
                    playerNamesList: uniquePlayerNames(row.player_names),
                };
            }),
        [noteValues, rows],
    );
    const [sortState, setSortState] = React.useState<SortState>({
        key: "last_seen",
        direction: "desc",
    });
    const [currentPage, setCurrentPage] = React.useState<number>(1);
    const [searchText, setSearchText] = React.useState<string>("");
    const [expandedPlayers, setExpandedPlayers] = React.useState<Set<string>>(
        () => new Set<string>(),
    );
    const filtered = React.useMemo(() => {
        const normalizedSearch = searchText.trim().toLowerCase();
        if (normalizedSearch === "") {
            return data;
        }

        return data.filter((row) => {
            const handle = asTableValueCompat(row.handle).toLowerCase();
            const player = asTableValueCompat(row.player).toLowerCase();
            const note = row.resolvedNote.toLowerCase();
            const playerNameMatch = row.playerNamesList.some((name) =>
                name.toLowerCase().includes(normalizedSearch),
            );
            return (
                handle.includes(normalizedSearch) ||
                player.includes(normalizedSearch) ||
                note.includes(normalizedSearch) ||
                playerNameMatch
            );
        });
    }, [data, searchText]);
    const sorted = React.useMemo(
        () =>
            sortRows(filtered, sortState, (row, key) => {
                if (key === "handle") return row.handle;
                if (key === "player") return row.player;
                if (key === "wins") return Number(row.wins || 0);
                if (key === "losses") return Number(row.losses || 0);
                if (key === "winrate") return Number(row.winrate || 0);
                if (key === "apm") return Number(row.apm || 0);
                if (key === "commander")
                    return languageManager.localize(row.commander);
                if (key === "frequency") return Number(row.frequency || 0);
                if (key === "kills") return Number(row.kills || 0);
                if (key === "last_seen") return Number(row.last_seen || 0);
                if (key === "note") return row.resolvedNote;
                return "";
            }),
        [filtered, languageManager, sortState],
    );
    const totalPages = pageCountForRows(sorted.length);

    React.useEffect(() => {
        setCurrentPage((page) => clampPageNumber(page, totalPages));
    }, [totalPages]);

    React.useEffect(() => {
        const visiblePlayers = new Set(filtered.map((row) => row.handleKey));
        setExpandedPlayers((current) => {
            const next = new Set<string>();
            for (const handleKey of current) {
                if (visiblePlayers.has(handleKey)) {
                    next.add(handleKey);
                }
            }
            return next.size === current.size ? current : next;
        });
    }, [filtered]);

    const pagedRows = React.useMemo(
        () => rowsForPage(sorted, currentPage),
        [currentPage, sorted],
    );

    const columns = [
        { key: "handle", label: t("ui_players_column_handle") },
        { key: "player", label: t("ui_players_column_player") },
        { key: "wins", label: t("ui_players_column_wins") },
        { key: "losses", label: t("ui_players_column_losses") },
        { key: "winrate", label: t("ui_players_column_winrate") },
        { key: "apm", label: t("ui_players_column_apm") },
        { key: "commander", label: t("ui_players_column_commander") },
        { key: "kills", label: t("ui_players_column_kill_percent") },
        { key: "last_seen", label: t("ui_players_column_last_seen") },
        { key: "note", label: t("ui_players_column_note") },
    ];

    function toggleExpanded(handleKey: string) {
        setExpandedPlayers((current) => {
            const next = new Set(current);
            if (next.has(handleKey)) {
                next.delete(handleKey);
            } else {
                next.add(handleKey);
            }
            return next;
        });
    }
    return (
        <div className="tab-content">
            <section className="card group games-panel">
                <div className="games-toolbar">
                    <h3>{t("ui_tab_players")}</h3>
                    <div className="games-toolbar-actions">
                        <input
                            type="text"
                            className="input games-search"
                            value={searchText}
                            onChange={(event) =>
                                setSearchText(event.currentTarget.value)
                            }
                            placeholder={t("ui_players_search_placeholder")}
                            aria-label={t("ui_players_search_placeholder")}
                        />
                        <button
                            type="button"
                            className="games-icon-btn"
                            onClick={onRefresh}
                            disabled={isBusy}
                            title={t("ui_common_refresh")}
                        >
                            {isBusy ? "..." : "🔄"}
                        </button>
                    </div>
                </div>
                <TablePagination
                    currentPage={currentPage}
                    onPageChange={setCurrentPage}
                    totalRows={sorted.length}
                />
                <div className="table-wrap" style={{ marginTop: "20px" }}>
                    <table className="data-table games-table">
                        <thead>
                            <tr>
                                {columns.map((column) => (
                                    <th key={`players-header-${column.key}`}>
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
                                        {t("ui_players_empty")}
                                    </td>
                                </tr>
                            ) : (
                                pagedRows.map((row, idx) => {
                                    const isExpanded = expandedPlayers.has(
                                        row.handleKey,
                                    );
                                    const canExpand =
                                        row.playerNamesList.length > 1;
                                    const toggleRow = () => {
                                        if (canExpand) {
                                            toggleExpanded(row.handleKey);
                                        }
                                    };
                                    const clickableClass = canExpand
                                        ? "players-row-clickable"
                                        : "";

                                    return (
                                        <React.Fragment
                                            key={`${row.handleKey}-${idx}`}
                                        >
                                            <tr
                                                className={
                                                    isExpanded
                                                        ? "players-summary-row is-expanded"
                                                        : "players-summary-row"
                                                }
                                            >
                                                <td
                                                    className={clickableClass}
                                                    onClick={toggleRow}
                                                >
                                                    {asTableValue(row.handle)}
                                                </td>
                                                <td
                                                    className={`player ${clickableClass}`.trim()}
                                                    onClick={toggleRow}
                                                >
                                                    <div className="players-player-cell">
                                                        <span className="players-player-name">
                                                            {asTableValue(
                                                                row.player,
                                                            )}
                                                        </span>
                                                        {canExpand ? (
                                                            <button
                                                                type="button"
                                                                className="players-expander-btn"
                                                                aria-label={
                                                                    isExpanded
                                                                        ? t(
                                                                              "ui_common_collapse",
                                                                          )
                                                                        : t(
                                                                              "ui_common_expand",
                                                                          )
                                                                }
                                                                onClick={(
                                                                    event,
                                                                ) => {
                                                                    event.stopPropagation();
                                                                    toggleExpanded(
                                                                        row.handleKey,
                                                                    );
                                                                }}
                                                            >
                                                                {isExpanded
                                                                    ? "-"
                                                                    : "+"}
                                                            </button>
                                                        ) : null}
                                                    </div>
                                                </td>
                                                <td
                                                    className={clickableClass}
                                                    onClick={toggleRow}
                                                >
                                                    {asTableValue(row.wins)}
                                                </td>
                                                <td
                                                    className={clickableClass}
                                                    onClick={toggleRow}
                                                >
                                                    {asTableValue(row.losses)}
                                                </td>
                                                <td
                                                    className={clickableClass}
                                                    onClick={toggleRow}
                                                >
                                                    {formatPercent(row.winrate)}
                                                </td>
                                                <td
                                                    className={clickableClass}
                                                    onClick={toggleRow}
                                                >
                                                    {asTableValue(
                                                        Math.round(
                                                            Number(
                                                                row.apm || 0,
                                                            ),
                                                        ),
                                                    )}
                                                </td>
                                                <td
                                                    className={clickableClass}
                                                    onClick={toggleRow}
                                                >
                                                    {languageManager.localize(
                                                        row.commander,
                                                    )}
                                                </td>
                                                <td
                                                    className={clickableClass}
                                                    onClick={toggleRow}
                                                >
                                                    {`${(Number(row.kills || 0) * 100).toFixed(1)}%`}
                                                </td>
                                                <td
                                                    className={clickableClass}
                                                    onClick={toggleRow}
                                                >
                                                    {formatReplayTime(
                                                        row.last_seen,
                                                    )}
                                                </td>
                                                <td>
                                                    <input
                                                        type="text"
                                                        className="input"
                                                        value={row.resolvedNote}
                                                        onChange={(event) =>
                                                            onNoteChange(
                                                                asTableValueCompat(
                                                                    row.handle,
                                                                ),
                                                                event
                                                                    .currentTarget
                                                                    .value,
                                                            )
                                                        }
                                                        onBlur={(event) => {
                                                            void onNoteCommit(
                                                                asTableValueCompat(
                                                                    row.handle,
                                                                ),
                                                                event
                                                                    .currentTarget
                                                                    .value,
                                                            );
                                                        }}
                                                        placeholder={t(
                                                            "ui_players_memo_placeholder",
                                                        )}
                                                        disabled={isBusy}
                                                    />
                                                </td>
                                            </tr>
                                            {isExpanded ? (
                                                <tr className="players-detail-row">
                                                    <td
                                                        colSpan={10}
                                                        className="players-detail-cell"
                                                    >
                                                        <div className="players-detail-block">
                                                            <span className="players-detail-label">
                                                                {t(
                                                                    "ui_players_usernames",
                                                                )}
                                                            </span>
                                                            <div className="players-handle-list">
                                                                {row.playerNamesList.map(
                                                                    (name) => (
                                                                        <code
                                                                            key={
                                                                                name
                                                                            }
                                                                            className="players-handle-chip"
                                                                        >
                                                                            {
                                                                                name
                                                                            }
                                                                        </code>
                                                                    ),
                                                                )}
                                                            </div>
                                                        </div>
                                                    </td>
                                                </tr>
                                            ) : null}
                                        </React.Fragment>
                                    );
                                })
                            )}
                        </tbody>
                    </table>
                </div>
                <TablePagination
                    currentPage={currentPage}
                    onPageChange={setCurrentPage}
                    totalRows={sorted.length}
                />
            </section>
        </div>
    );
}
