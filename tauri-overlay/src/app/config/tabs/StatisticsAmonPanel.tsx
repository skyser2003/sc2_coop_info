import type * as React from "react";
import type { StatsAmonUnitRow } from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import type { StatisticsAnalysis, StatisticsPayload } from "../types";
import { sortRows, type SortState } from "./tableSort";
import { tableHeader } from "./statisticsTable";
import { formatNumber, readNumber, translate } from "./statisticsViewModels";
import styles from "../page.module.css";

type StatisticsAmonPanelProps = {
    analysis: StatisticsAnalysis;
    statsPayload: StatisticsPayload | null;
    amonSort: SortState;
    onAmonSort: (key: string) => void;
    languageManager: LanguageManager;
};

export default function StatisticsAmonPanel({
    analysis,
    statsPayload,
    amonSort,
    onAmonSort,
    languageManager,
}: StatisticsAmonPanelProps): React.ReactNode {
    const unitData = analysis.UnitData;
    const detailNote = translate(
        languageManager,
        "ui_stats_detailed_stats_note",
        {
            detailed: readNumber(statsPayload?.detailed_parsed_count, 0),
            total: readNumber(statsPayload?.total_valid_files, 0),
        },
    );
    if (!unitData || !unitData.amon) {
        return (
            <div className={styles.statsDetailEmpty}>
                <p>{detailNote}</p>
                {languageManager.translate("ui_stats_amon_requires_full")}
            </div>
        );
    }

    const rowsBase = Object.entries(unitData.amon);
    rowsBase.sort((a, b) => {
        if (a[0] === "sum") return -1;
        if (b[0] === "sum") return 1;
        const createdDelta =
            Number(b[1].created || 0) - Number(a[1].created || 0);
        if (createdDelta !== 0) return createdDelta;
        return languageManager
            .localizeUnitName(String(a[0]))
            .localeCompare(languageManager.localizeUnitName(String(b[0])));
    });
    const sumRow = rowsBase.find(([name]) => name === "sum") || null;
    const detailRowsBase = rowsBase.filter(([name]) => name !== "sum");
    const detailRows = sortRows(
        detailRowsBase,
        amonSort,
        ([name, row], key) => {
            if (key === "name") return languageManager.localizeUnitName(name);
            if (key === "created") return Number(row.created || 0);
            if (key === "lost") return Number(row.lost || 0);
            if (key === "kills") return Number(row.kills || 0);
            if (key === "kd") return sortAmonKd(row);
            return "";
        },
    );
    const rows = sumRow ? [sumRow, ...detailRows] : detailRows;

    return (
        <div className={styles.statsSubContent}>
            <p className={styles.note}>{detailNote}</p>
            <div className={styles.tableWrap}>
                <table
                    className={[styles.dataTable, styles.statsDense]
                        .filter(Boolean)
                        .join(" ")}
                >
                    {tableHeader(
                        [
                            {
                                key: "name",
                                label: languageManager.translate(
                                    "ui_stats_name",
                                ),
                            },
                            {
                                key: "created",
                                label: languageManager.translate(
                                    "ui_stats_created",
                                ),
                            },
                            {
                                key: "lost",
                                label: languageManager.translate(
                                    "ui_stats_lost",
                                ),
                            },
                            {
                                key: "kills",
                                label: languageManager.translate(
                                    "ui_stats_kills",
                                ),
                            },
                            { key: "kd", label: "K/D" },
                        ],
                        amonSort,
                        onAmonSort,
                    )}
                    <tbody>
                        {rows.map(([name, row]) => (
                            <tr
                                key={`amon-${name}`}
                                className={
                                    name === "sum" ? styles.statsSumRow : ""
                                }
                            >
                                <td>
                                    {name === "sum"
                                        ? languageManager.translate(
                                              "ui_common_total",
                                          )
                                        : languageManager.localizeUnitName(
                                              name,
                                          )}
                                </td>
                                <td>{formatNumber(row.created)}</td>
                                <td>{formatNumber(row.lost)}</td>
                                <td>{formatNumber(row.kills)}</td>
                                <td>{formatAmonKd(row)}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </div>
    );
}

function sortAmonKd(row: StatsAmonUnitRow): number {
    const raw = row.KD;
    if (typeof raw === "string") {
        if (raw.toLowerCase() === "inf") {
            return Number.POSITIVE_INFINITY;
        }
        const parsed = Number(raw);
        return Number.isFinite(parsed) ? parsed : 0;
    }
    return Number(raw || 0);
}

function formatAmonKd(row: StatsAmonUnitRow): string {
    return typeof row.KD === "string" ? row.KD : Number(row.KD || 0).toFixed(1);
}
