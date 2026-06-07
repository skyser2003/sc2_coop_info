import type * as React from "react";
import type {
    StatsCommanderUnitRow,
    StatsUnitDataPayload,
} from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import type {
    StatisticsAnalysis,
    StatisticsPayload,
    StatisticsState,
    StatsHelpers,
} from "../types";
import {
    finiteNumberOrNull,
    formatNumber,
    formatPercent0,
    formatPercent1,
    readNumber,
    translate,
} from "./statisticsViewModels";
import styles from "../configStyles";

type UnitSide = "main" | "ally";
type UnitCommanderStats = NonNullable<StatsUnitDataPayload[UnitSide][string]>;
type UnitRowEntry = readonly [string, StatsCommanderUnitRow];
type UnitSortField = keyof StatsCommanderUnitRow | "Name";
type UnitCommanderEntry = {
    readonly name: string;
    readonly hasUnitRecord: boolean;
};

type StatisticsUnitsPanelProps = {
    analysis: StatisticsAnalysis;
    statsPayload: StatisticsPayload | null;
    statsState: StatisticsState;
    actions: StatsHelpers;
    languageManager: LanguageManager;
};

export default function StatisticsUnitsPanel({
    analysis,
    statsPayload,
    statsState,
    actions,
    languageManager,
}: StatisticsUnitsPanelProps): React.ReactNode {
    const unitData = analysis.UnitData;
    const detailNote = translate(
        languageManager,
        "ui_stats_detailed_stats_note",
        {
            detailed: readNumber(statsPayload?.detailed_parsed_count, 0),
            total: readNumber(statsPayload?.total_valid_files, 0),
        },
    );
    if (!unitData) {
        return (
            <div className={styles.statsDetailEmpty}>
                <p>{detailNote}</p>
                {languageManager.translate("ui_stats_units_requires_full")}
            </div>
        );
    }

    const mainCommanders = UnitCommanderListBuilder.entries(
        statsPayload,
        analysis,
        unitData,
        "main",
        languageManager,
    );
    const allyCommanders = UnitCommanderListBuilder.entries(
        statsPayload,
        analysis,
        unitData,
        "ally",
        languageManager,
    );
    const mainCommander = UnitCommanderListBuilder.selectedCommander(
        statsState.selectedUnitMainCommander,
        mainCommanders,
    );
    const allyCommander = UnitCommanderListBuilder.selectedCommander(
        statsState.selectedUnitAllyCommander,
        allyCommanders,
    );
    const side = statsState.selectedUnitSide || "main";
    const commander = side === "main" ? mainCommander : allyCommander;
    const source = commanderStatsFor(unitData, side, commander);
    const games = Number(source?.count || 0);
    const defaultUnitSort = languageManager.translate("ui_stats_unit");
    const sortBy = statsState.selectedUnitSortBy || defaultUnitSort;
    const sortReverse =
        typeof statsState.selectedUnitSortReverse === "boolean"
            ? statsState.selectedUnitSortReverse
            : false;

    const applyUnitSort = (field: string) =>
        actions.setStatsState((current) => {
            const currentField = current.selectedUnitSortBy || defaultUnitSort;
            const currentReverse =
                typeof current.selectedUnitSortReverse === "boolean"
                    ? current.selectedUnitSortReverse
                    : false;

            if (currentField === field) {
                return {
                    ...current,
                    selectedUnitSortReverse: !currentReverse,
                };
            }

            const defaultReverse = field === defaultUnitSort;
            return {
                ...current,
                selectedUnitSortBy: field,
                selectedUnitSortReverse: !defaultReverse,
            };
        });

    const sortFieldByHeader: { readonly [label: string]: UnitSortField } = {
        [languageManager.translate("ui_stats_unit")]: "Name",
        [languageManager.translate("ui_stats_created")]: "created",
        [languageManager.translate("ui_stats_freq")]: "made",
        [languageManager.translate("ui_stats_lost")]: "lost",
        [languageManager.translate("ui_stats_lost_percent")]: "lost_percent",
        [languageManager.translate("ui_stats_kills")]: "kills",
        "K/D": "KD",
        [languageManager.translate("ui_stats_kills_percent")]:
            "kill_percentage",
    };
    const orderedEntries = orderUnitRows(
        unitRowsFor(source),
        sortBy,
        sortReverse,
        defaultUnitSort,
        sortFieldByHeader,
        languageManager,
    );
    const rows = filteredUnitRows(orderedEntries, commander);

    const sortHeaderText = (field: string) => {
        if (sortBy !== field) {
            return field;
        }
        const arrow = sortReverse ? "▼" : "▲";
        return field === defaultUnitSort
            ? `${field}${arrow}`
            : `${arrow}${field}`;
    };

    return (
        <div
            className={[styles.statsSubContent, styles.statsUnitsLayout]
                .filter(Boolean)
                .join(" ")}
        >
            <div
                className={[
                    styles.statsUnitSelectors,
                    styles.statsUnitCommanders,
                ]
                    .filter(Boolean)
                    .join(" ")}
            >
                <CommanderPicker
                    active={side === "main"}
                    commander={mainCommander}
                    commanders={mainCommanders}
                    keyPrefix="main"
                    languageManager={languageManager}
                    title={languageManager.translate("ui_stats_side_main")}
                    onSelect={(name) =>
                        actions.setStatsState((current) => ({
                            ...current,
                            selectedUnitMainCommander: name,
                            selectedUnitSide: "main",
                        }))
                    }
                />
                <CommanderPicker
                    active={side === "ally"}
                    commander={allyCommander}
                    commanders={allyCommanders}
                    keyPrefix="ally"
                    languageManager={languageManager}
                    title={languageManager.translate("ui_stats_side_ally")}
                    onSelect={(name) =>
                        actions.setStatsState((current) => ({
                            ...current,
                            selectedUnitAllyCommander: name,
                            selectedUnitSide: "ally",
                        }))
                    }
                />
            </div>
            <div className={styles.statsUnitTable}>
                <h3>
                    {translate(languageManager, "ui_stats_unit_stats_title", {
                        side: languageManager.translate(
                            side === "main"
                                ? "ui_stats_side_main"
                                : "ui_stats_side_ally",
                        ),
                        commander: commander
                            ? languageManager.localize(commander)
                            : "-",
                    })}
                </h3>
                <p className={styles.note}>{detailNote}</p>
                <div className={styles.tableWrap}>
                    <table
                        className={[
                            styles.dataTable,
                            styles.statsDense,
                            styles.statsUnitTableGrid,
                        ]
                            .filter(Boolean)
                            .join(" ")}
                    >
                        <colgroup>
                            <col key="unit-col-1" />
                            <col key="unit-col-2" />
                            <col key="unit-col-3" />
                            <col key="unit-col-4" />
                            <col key="unit-col-5" />
                            <col key="unit-col-6" />
                            <col key="unit-col-7" />
                            <col key="unit-col-8" />
                        </colgroup>
                        <thead>
                            <tr>
                                <th>
                                    <button
                                        type="button"
                                        className={styles.statsSortBtn}
                                        onClick={() =>
                                            applyUnitSort(defaultUnitSort)
                                        }
                                    >
                                        {sortHeaderText(defaultUnitSort)}
                                    </button>
                                </th>
                                {unitStatisticHeaders(languageManager).map(
                                    (field) => (
                                        <th key={`unit-header-${field}`}>
                                            <button
                                                type="button"
                                                className={[
                                                    styles.statsSortBtn,
                                                    styles.statsSortBtnRight,
                                                ]
                                                    .filter(Boolean)
                                                    .join(" ")}
                                                onClick={() =>
                                                    applyUnitSort(field)
                                                }
                                            >
                                                {sortHeaderText(field)}
                                            </button>
                                        </th>
                                    ),
                                )}
                            </tr>
                        </thead>
                        <tbody>
                            {rows.map(([name, row]) => (
                                <tr
                                    key={`unit-${side}-${commander}-${name}`}
                                    className={
                                        name === "sum" ? styles.statsSumRow : ""
                                    }
                                >
                                    <td className={styles.statsUnitColName}>
                                        {name === "sum"
                                            ? `Σ (${formatNumber(games)} ${languageManager.translate("ui_stats_games_suffix")})`
                                            : languageManager.localizeUnitName(
                                                  name,
                                              )}
                                    </td>
                                    <td className={styles.statsUnitColNum}>
                                        {formatNumber(row.created)}
                                    </td>
                                    <td className={styles.statsUnitColNum}>
                                        {formatPercent0(row.made)}
                                    </td>
                                    <td className={styles.statsUnitColNum}>
                                        {formatNumber(row.lost)}
                                    </td>
                                    <td className={styles.statsUnitColNum}>
                                        {row.lost_percent === null
                                            ? "-"
                                            : formatPercent0(row.lost_percent)}
                                    </td>
                                    <td className={styles.statsUnitColNum}>
                                        {formatNumber(row.kills)}
                                    </td>
                                    <td className={styles.statsUnitColNum}>
                                        {row.KD === null
                                            ? "-"
                                            : Number(row.KD).toFixed(1)}
                                    </td>
                                    <td className={styles.statsUnitColNum}>
                                        {formatPercent1(row.kill_percentage)}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
                <p
                    className={[styles.note, styles.statsRightNote]
                        .filter(Boolean)
                        .join(" ")}
                >
                    {languageManager.translate("ui_stats_mind_control_note")}
                </p>
            </div>
        </div>
    );
}

function CommanderPicker({
    active,
    commander,
    commanders,
    keyPrefix,
    languageManager,
    onSelect,
    title,
}: {
    active: boolean;
    commander: string;
    commanders: readonly UnitCommanderEntry[];
    keyPrefix: string;
    languageManager: LanguageManager;
    onSelect: (name: string) => void;
    title: string;
}): React.ReactNode {
    return (
        <div className={styles.statsUnitColumn}>
            <h4>{title}</h4>
            <div className={styles.tableWrap}>
                <table
                    className={[
                        styles.dataTable,
                        styles.statsDense,
                        styles.statsUnitPickerTable,
                    ]
                        .filter(Boolean)
                        .join(" ")}
                >
                    <tbody>
                        {commanders.map((entry) => (
                            <tr
                                key={`${keyPrefix}-${entry.name}`}
                                className={[
                                    active && commander === entry.name
                                        ? styles.selectedRow
                                        : "",
                                    entry.hasUnitRecord
                                        ? ""
                                        : styles.statsUnitCommanderDisabled,
                                ]
                                    .filter(Boolean)
                                    .join(" ")}
                                aria-disabled={!entry.hasUnitRecord}
                                data-testid={`unit-commander-${keyPrefix}-${entry.name}`}
                                onClick={
                                    entry.hasUnitRecord
                                        ? () => onSelect(entry.name)
                                        : undefined
                                }
                            >
                                <td>{languageManager.localize(entry.name)}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </div>
    );
}

function commanderStatsFor(
    unitData: StatsUnitDataPayload,
    side: UnitSide,
    commander: string,
): UnitCommanderStats | null {
    if (!commander) {
        return null;
    }

    return unitData[side][commander] ?? null;
}

class UnitCommanderListBuilder {
    static entries(
        statsPayload: StatisticsPayload | null,
        analysis: StatisticsAnalysis,
        unitData: StatsUnitDataPayload,
        side: UnitSide,
        languageManager: LanguageManager,
    ): UnitCommanderEntry[] {
        const names = new Set<string>();
        const sideCommanderData =
            side === "main"
                ? analysis.CommanderData
                : analysis.AllyCommanderData;

        UnitCommanderListBuilder.addCommanderNames(
            names,
            Object.keys(statsPayload?.prestige_names ?? {}),
        );
        UnitCommanderListBuilder.addCommanderNames(
            names,
            Object.keys(languageManager.commanderMasteryData()),
        );
        UnitCommanderListBuilder.addCommanderNames(
            names,
            Object.keys(sideCommanderData),
        );
        UnitCommanderListBuilder.addCommanderNames(
            names,
            Object.keys(unitData[side] ?? {}),
        );

        return [...names]
            .sort((left, right) =>
                languageManager
                    .localize(left)
                    .localeCompare(languageManager.localize(right)),
            )
            .map((name) => ({
                name,
                hasUnitRecord: UnitCommanderListBuilder.hasUnitRecord(
                    commanderStatsFor(unitData, side, name),
                ),
            }));
    }

    static selectedCommander(
        requestedCommander: string,
        commanders: readonly UnitCommanderEntry[],
    ): string {
        const requested = commanders.find(
            (entry) => entry.name === requestedCommander && entry.hasUnitRecord,
        );
        if (requested) {
            return requested.name;
        }

        return commanders.find((entry) => entry.hasUnitRecord)?.name ?? "";
    }

    private static addCommanderNames(
        target: Set<string>,
        names: readonly string[],
    ): void {
        for (const name of names) {
            const trimmed = name.trim();
            if (trimmed !== "" && trimmed !== "any") {
                target.add(trimmed);
            }
        }
    }

    private static hasUnitRecord(source: UnitCommanderStats | null): boolean {
        return unitRowsFor(source).some(([name]) => name !== "sum");
    }
}

function isUnitStatRow(
    row: StatsCommanderUnitRow | number,
): row is StatsCommanderUnitRow {
    return typeof row === "object" && row !== null;
}

function unitRowsFor(source: UnitCommanderStats | null): UnitRowEntry[] {
    if (!source) {
        return [];
    }

    return Object.entries(source).filter(
        (entry): entry is [string, StatsCommanderUnitRow] =>
            entry[0] !== "count" && isUnitStatRow(entry[1]),
    );
}

function unitStatisticHeaders(languageManager: LanguageManager): string[] {
    return [
        languageManager.translate("ui_stats_created"),
        languageManager.translate("ui_stats_freq"),
        languageManager.translate("ui_stats_lost"),
        languageManager.translate("ui_stats_lost_percent"),
        languageManager.translate("ui_stats_kills"),
        "K/D",
        languageManager.translate("ui_stats_kills_percent"),
    ];
}

function orderUnitRows(
    entries: UnitRowEntry[],
    sortBy: string,
    sortReverse: boolean,
    defaultUnitSort: string,
    sortFieldByHeader: { readonly [label: string]: UnitSortField },
    languageManager: LanguageManager,
): UnitRowEntry[] {
    const sorted = [...entries];
    if (sortBy === defaultUnitSort) {
        sorted.sort((a, b) =>
            sortReverse
                ? languageManager
                      .localizeUnitName(b[0])
                      .localeCompare(languageManager.localizeUnitName(a[0]))
                : languageManager
                      .localizeUnitName(a[0])
                      .localeCompare(languageManager.localizeUnitName(b[0])),
        );
        return sorted;
    }

    const field = sortFieldByHeader[sortBy] || "Name";
    sorted.sort((a, b) => {
        const va = sortableUnitValue(a[1], field);
        const vb = sortableUnitValue(b[1], field);
        if (va === vb) return 0;
        return sortReverse ? vb - va : va - vb;
    });
    return sorted;
}

function sortableUnitValue(row: StatsCommanderUnitRow, field: UnitSortField) {
    if (field === "Name") {
        return 0;
    }

    const value = row[field];
    if (typeof value === "number" && Number.isFinite(value)) {
        return value;
    }
    return 0;
}

function filteredUnitRows(
    orderedEntries: UnitRowEntry[],
    commander: string,
): UnitRowEntry[] {
    const hiddenMindControlUnits =
        commander === "Tychus" ||
        commander === "Vorazun" ||
        commander === "Zeratul" ||
        commander === "Abathur";
    const filteredRows = orderedEntries.filter(([name, row]) => {
        if (name === "count") return false;

        if (
            name === "Primal Hive" ||
            name === "Primal Warden" ||
            name === "Archangel"
        ) {
            return false;
        }

        if (
            (commander === "Karax" && name === "Disruptor") ||
            (commander !== "Stukov" && name === "Brood Queen") ||
            (commander !== "Tychus" && name === "Auto-Turret")
        ) {
            return false;
        }

        if (
            hiddenMindControlUnits &&
            (name === "Broodling" || name === "Infested Terran")
        ) {
            return false;
        }

        const created = finiteNumberOrNull(row.created);
        if (created !== null) {
            return created > 0;
        }

        return (
            (finiteNumberOrNull(row.made) ?? 0) > 0 ||
            (finiteNumberOrNull(row.kills) ?? 0) > 0 ||
            (finiteNumberOrNull(row.lost) ?? 0) > 0
        );
    });
    const sumEntry = filteredRows.find(([name]) => name === "sum");
    const unitRows = filteredRows.filter(([name]) => name !== "sum");
    return sumEntry ? [...unitRows, sumEntry] : unitRows;
}
