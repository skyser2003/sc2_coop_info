import * as React from "react";
import type { LanguageManager } from "../../i18n/languageManager";
import type {
    DisplayValue,
    JsonValue,
    LocalizedText,
    MutatorData,
} from "../types";
import {
    nextSortState,
    sortIndicator,
    sortRows,
    type SortState,
} from "./tableSort";

type WeekliesTabRow = {
    mutation?: string | null;
    nameEn?: string | null;
    nameKo?: string | null;
    map?: string | null;
    mutators?: readonly MutatorData[] | null;
    mutationOrder?: number | string | null;
    isCurrent?: boolean | null;
    nextDuration?: string | null;
    nextDurationDays?: number | string | null;
    difficulty?: string | null;
    wins?: number | string | null;
    losses?: number | string | null;
    winrate?: number | string | null;
};

type WeekliesTabProps = {
    rows: readonly WeekliesTabRow[] | null;
    onRefresh: () => void;
    isBusy: boolean;
    asTableValue?: (value: DisplayValue) => string;
    formatPercent?: (value: DisplayValue) => string;
    languageManager: LanguageManager;
};

function asTableValueCompat(value: DisplayValue): string {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

function formatPercentCompat(value: DisplayValue): string {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "0.0%";
    }
    return `${(num * 100).toFixed(1)}%`;
}

function mutatorIconPath(iconName: string): string {
    return `/overlay/Mutator Icons/${encodeURIComponent(iconName)}.png`;
}

function isWeekliesTabMutator(value: JsonValue): value is MutatorData {
    return value !== null && typeof value === "object" && !Array.isArray(value);
}

function readMutators(value: DisplayValue): readonly MutatorData[] {
    if (!Array.isArray(value)) {
        return [];
    }

    return value.filter(isWeekliesTabMutator);
}

function mutationKeyForRow(row: WeekliesTabRow): string {
    return typeof row.mutation === "string" ? row.mutation : "";
}

function localizedWeeklyMutationName(
    row: WeekliesTabRow,
    languageManager: LanguageManager,
    asTableValue: (value: DisplayValue) => string,
): string {
    const preferred =
        languageManager.currentLanguage() === "ko" ? row.nameKo : row.nameEn;
    const fallback =
        languageManager.currentLanguage() === "ko" ? row.nameEn : row.nameKo;
    const localizedName = asTableValue(preferred || fallback);
    if (localizedName !== "") {
        return localizedName;
    }

    return languageManager.localize(row.mutation);
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

function localizeWeeklyDuration(
    value: DisplayValue,
    languageManager: LanguageManager,
    asTableValue: (value: DisplayValue) => string,
): string {
    const text = asTableValue(value).trim();
    if (text === "") {
        return "";
    }

    if (text === "Now") {
        return languageManager.translate("ui_weeklies_now");
    }

    const localizedParts = text.split(/\s+/).map((part) => {
        const match = /^(\d+)([wd])$/.exec(part);
        if (!match) {
            return part;
        }

        const [, amount, unit] = match;
        const unitId =
            unit === "w"
                ? "ui_weeklies_duration_week_suffix"
                : "ui_weeklies_duration_day_suffix";
        return `${amount}${languageManager.translate(unitId)}`;
    });

    return localizedParts.join(" ");
}

export default function WeekliesTab({
    rows,
    onRefresh,
    isBusy,
    asTableValue = asTableValueCompat,
    formatPercent = formatPercentCompat,
    languageManager,
}: WeekliesTabProps) {
    const t = (id: string): string => languageManager.translate(id);
    const data: readonly WeekliesTabRow[] = Array.isArray(rows) ? rows : [];
    const [sortState, setSortState] = React.useState<SortState>({
        key: "nextDuration",
        direction: "asc",
    });
    const [selectedMutation, setSelectedMutation] = React.useState<string>("");

    const sorted = React.useMemo(
        () =>
            sortRows(data, sortState, (row, key) => {
                if (key === "mutation") {
                    return localizedWeeklyMutationName(
                        row,
                        languageManager,
                        asTableValue,
                    );
                }
                if (key === "nextDuration") {
                    const nextDurationDays = Number(row.nextDurationDays);
                    if (Number.isFinite(nextDurationDays)) {
                        return nextDurationDays;
                    }
                    return Number.MAX_SAFE_INTEGER;
                }
                if (key === "difficulty") {
                    return languageManager.localize(row.difficulty);
                }
                if (key === "wins") return Number(row.wins || 0);
                if (key === "losses") return Number(row.losses || 0);
                if (key === "winrate") return Number(row.winrate || 0);
                return "";
            }),
        [asTableValue, data, languageManager, sortState],
    );

    React.useEffect(() => {
        if (sorted.length === 0) {
            if (selectedMutation !== "") {
                setSelectedMutation("");
            }
            return;
        }

        const hasSelected = sorted.some(
            (row) => mutationKeyForRow(row) === selectedMutation,
        );
        if (hasSelected) {
            return;
        }

        const currentRow =
            sorted.find((row) => row.isCurrent === true) ?? sorted[0];
        setSelectedMutation(mutationKeyForRow(currentRow));
    }, [selectedMutation, sorted]);

    const selectedRow = React.useMemo(() => {
        if (sorted.length === 0) {
            return null;
        }

        return (
            sorted.find((row) => mutationKeyForRow(row) === selectedMutation) ??
            sorted.find((row) => row.isCurrent === true) ??
            sorted[0]
        );
    }, [selectedMutation, sorted]);

    const selectedMutators = React.useMemo(
        () => readMutators(selectedRow?.mutators),
        [selectedRow],
    );

    const columns = [
        { key: "mutation", label: t("ui_weeklies_column_mutation") },
        { key: "nextDuration", label: t("ui_weeklies_column_next_in") },
        { key: "difficulty", label: t("ui_weeklies_column_best_difficulty") },
        { key: "wins", label: t("ui_weeklies_column_wins") },
        { key: "losses", label: t("ui_weeklies_column_losses") },
        { key: "winrate", label: t("ui_weeklies_column_winrate") },
    ];

    return (
        <div className="tab-content">
            <section className="card group games-panel">
                <div className="games-toolbar">
                    <h3>{t("ui_tab_weeklies")}</h3>
                    <div className="games-toolbar-actions">
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
                <div className="weeklies-layout">
                    <div className="weeklies-table-pane table-wrap">
                        <table className="data-table games-table">
                            <thead>
                                <tr>
                                    {columns.map((column) => (
                                        <th
                                            key={`weeklies-header-${column.key}`}
                                        >
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
                                        <td colSpan={6} className="empty-cell">
                                            {t("ui_weeklies_empty")}
                                        </td>
                                    </tr>
                                ) : (
                                    sorted.map((row, idx) => {
                                        const mutationKey =
                                            mutationKeyForRow(row);
                                        const isSelected =
                                            mutationKey === selectedMutation;
                                        return (
                                            <tr
                                                key={`${mutationKey}-${idx}`}
                                                className={`weeklies-row-clickable${isSelected ? " selected-row" : ""}`}
                                                onClick={() =>
                                                    setSelectedMutation(
                                                        mutationKey,
                                                    )
                                                }
                                            >
                                                <td>
                                                    <div className="weeklies-name-cell">
                                                        <span>
                                                            {localizedWeeklyMutationName(
                                                                row,
                                                                languageManager,
                                                                asTableValue,
                                                            )}
                                                        </span>
                                                        {row.isCurrent ===
                                                        true ? (
                                                            <span className="weeklies-current-pill">
                                                                {languageManager.translate(
                                                                    "ui_weeklies_now",
                                                                )}
                                                            </span>
                                                        ) : null}
                                                    </div>
                                                </td>
                                                <td>
                                                    {localizeWeeklyDuration(
                                                        row.nextDuration,
                                                        languageManager,
                                                        asTableValue,
                                                    )}
                                                </td>
                                                <td>
                                                    {languageManager.localize(
                                                        row.difficulty,
                                                    )}
                                                </td>
                                                <td>
                                                    {asTableValue(row.wins)}
                                                </td>
                                                <td>
                                                    {asTableValue(row.losses)}
                                                </td>
                                                <td>
                                                    {formatPercent(row.winrate)}
                                                </td>
                                            </tr>
                                        );
                                    })
                                )}
                            </tbody>
                        </table>
                    </div>
                    <aside className="weeklies-detail-pane">
                        {selectedRow === null ? (
                            <div className="stats-detail-empty">
                                {t("ui_weeklies_empty")}
                            </div>
                        ) : (
                            <div className="weeklies-detail-card">
                                <div className="weeklies-detail-head">
                                    <div>
                                        <h4 className="weeklies-detail-title">
                                            {localizedWeeklyMutationName(
                                                selectedRow,
                                                languageManager,
                                                asTableValue,
                                            )}
                                        </h4>
                                        <p className="weeklies-map">
                                            {languageManager.localize(
                                                selectedRow.map,
                                            )}
                                        </p>
                                    </div>
                                    {selectedRow.isCurrent === true ? (
                                        <span className="weeklies-current-pill">
                                            {t("ui_weeklies_current")}
                                        </span>
                                    ) : null}
                                </div>
                                <div className="weeklies-meta-row">
                                    <span className="weeklies-stat-chip">
                                        {`${t("ui_weeklies_column_next_in")}: ${localizeWeeklyDuration(
                                            selectedRow.nextDuration,
                                            languageManager,
                                            asTableValue,
                                        )}`}
                                    </span>
                                    <span className="weeklies-stat-chip">
                                        {`${t("ui_weeklies_column_best_difficulty")}: ${languageManager.localize(selectedRow.difficulty)}`}
                                    </span>
                                    <span className="weeklies-stat-chip">
                                        {`${t("ui_weeklies_column_winrate")}: ${formatPercent(selectedRow.winrate)}`}
                                    </span>
                                </div>
                                <div className="weeklies-mutator-grid">
                                    {selectedMutators.length === 0 ? (
                                        <div className="stats-detail-empty">
                                            No mutator details available.
                                        </div>
                                    ) : (
                                        selectedMutators.map(
                                            (mutator, index) => {
                                                const iconName = asTableValue(
                                                    mutator.iconName ||
                                                        mutator.name?.en ||
                                                        mutator.id ||
                                                        "",
                                                );
                                                const displayName =
                                                    localizedMutatorName(
                                                        mutator,
                                                        languageManager,
                                                        asTableValue,
                                                    );
                                                return (
                                                    <article
                                                        key={`${asTableValue(mutator.id || mutator.name?.en || "")}-${index}`}
                                                        className="weeklies-mutator-card"
                                                    >
                                                        <img
                                                            className="weeklies-mutator-icon"
                                                            src={mutatorIconPath(
                                                                iconName,
                                                            )}
                                                            alt={displayName}
                                                        />
                                                        <div className="weeklies-mutator-copy">
                                                            <h5 className="weeklies-mutator-name">
                                                                {displayName}
                                                            </h5>
                                                            <p className="weeklies-mutator-description">
                                                                {localizedMutatorDescription(
                                                                    mutator,
                                                                    languageManager,
                                                                    asTableValue,
                                                                )}
                                                            </p>
                                                        </div>
                                                    </article>
                                                );
                                            },
                                        )
                                    )}
                                </div>
                            </div>
                        )}
                    </aside>
                </div>
            </section>
        </div>
    );
}
