import type * as React from "react";
import type { LanguageManager } from "../../i18n/languageManager";
import type { StatisticsAnalysis } from "../types";
import { sortRows, type SortState } from "./tableSort";
import { tableHeader } from "./statisticsTable";
import {
    difficultySortRank,
    formatNumber,
    formatPercent0,
    orderedDifficultyRows,
    readNumber,
    regionStatsRows,
} from "./statisticsViewModels";
import styles from "../page.module.css";

type StatisticsDiffRegionPanelProps = {
    analysis: StatisticsAnalysis;
    regionSort: SortState;
    onRegionSort: (key: string) => void;
    difficultySort: SortState;
    onDifficultySort: (key: string) => void;
    languageManager: LanguageManager;
};

export default function StatisticsDiffRegionPanel({
    analysis,
    regionSort,
    onRegionSort,
    difficultySort,
    onDifficultySort,
    languageManager,
}: StatisticsDiffRegionPanelProps): React.ReactNode {
    const regionData = sortRows(
        regionStatsRows(analysis),
        regionSort,
        ([region, row], key) => {
            if (key === "region") return region;
            if (key === "frequency") return readNumber(row.frequency);
            if (key === "wins") return readNumber(row.Victory);
            if (key === "losses") return readNumber(row.Defeat);
            if (key === "winrate") return readNumber(row.winrate);
            if (key === "asc") return readNumber(row.max_asc);
            if (key === "prestiges") {
                return Object.values(row.prestiges).reduce<number>(
                    (sum, value) => sum + readNumber(value),
                    0,
                );
            }
            if (key === "maxed") {
                return row.max_com.length;
            }
            return "";
        },
    );
    const diffEntries = sortRows(
        orderedDifficultyRows(analysis),
        difficultySort,
        ([difficulty, row], key) => {
            if (key === "difficulty")
                return difficultySortRank(difficulty, languageManager);
            if (key === "wins") return readNumber(row.Victory);
            if (key === "losses") return readNumber(row.Defeat);
            if (key === "winrate") return readNumber(row.Winrate);
            return "";
        },
    );
    const diffTotals = diffEntries.reduce(
        (acc, [, row]) => {
            acc.wins += readNumber(row.Victory);
            acc.losses += readNumber(row.Defeat);
            return acc;
        },
        { wins: 0, losses: 0 },
    );
    const diffTotalGames = diffTotals.wins + diffTotals.losses;

    return (
        <div className={styles.statsSubContent}>
            <div className={styles.tableWrap}>
                <table
                    className={[styles.dataTable, styles.statsDense]
                        .filter(Boolean)
                        .join(" ")}
                >
                    {tableHeader(
                        [
                            {
                                key: "region",
                                label: languageManager.translate(
                                    "ui_stats_region",
                                ),
                            },
                            {
                                key: "frequency",
                                label: languageManager.translate(
                                    "ui_stats_frequency",
                                ),
                            },
                            {
                                key: "wins",
                                label: languageManager.translate(
                                    "ui_stats_wins",
                                ),
                            },
                            {
                                key: "losses",
                                label: languageManager.translate(
                                    "ui_stats_losses",
                                ),
                            },
                            {
                                key: "winrate",
                                label: languageManager.translate(
                                    "ui_stats_winrate",
                                ),
                            },
                            {
                                key: "asc",
                                label: languageManager.translate(
                                    "ui_stats_ascension_level",
                                ),
                            },
                            {
                                key: "prestiges",
                                label: languageManager.translate(
                                    "ui_stats_prestiges_unlocked",
                                ),
                            },
                            {
                                key: "maxed",
                                label: languageManager.translate(
                                    "ui_stats_maxed_commanders",
                                ),
                            },
                        ],
                        regionSort,
                        onRegionSort,
                    )}
                    <tbody>
                        {regionData.map(([region, row]) => {
                            const prestigeCount = Object.values(
                                row.prestiges,
                            ).reduce<number>(
                                (sum, value) => sum + readNumber(value),
                                0,
                            );
                            return (
                                <tr key={`region-${region}`}>
                                    <td>{region}</td>
                                    <td>{formatPercent0(row.frequency)}</td>
                                    <td>{formatNumber(row.Victory)}</td>
                                    <td>{formatNumber(row.Defeat)}</td>
                                    <td>{formatPercent0(row.winrate)}</td>
                                    <td>{formatNumber(row.max_asc)}</td>
                                    <td>{`${prestigeCount}/54`}</td>
                                    <td>
                                        {row.max_com.length >= 4
                                            ? `${row.max_com.length}/18`
                                            : row.max_com
                                                  .map((name) =>
                                                      languageManager.localize(
                                                          name,
                                                      ),
                                                  )
                                                  .join(", ")}
                                    </td>
                                </tr>
                            );
                        })}
                    </tbody>
                </table>
            </div>
            <div
                className={[styles.statsDiffWrap, styles.tableWrap]
                    .filter(Boolean)
                    .join(" ")}
            >
                <table
                    className={[
                        styles.dataTable,
                        styles.statsDense,
                        styles.statsNarrow,
                    ]
                        .filter(Boolean)
                        .join(" ")}
                >
                    {tableHeader(
                        [
                            {
                                key: "difficulty",
                                label: languageManager.translate(
                                    "ui_stats_difficulty",
                                ),
                            },
                            {
                                key: "wins",
                                label: languageManager.translate(
                                    "ui_stats_wins",
                                ),
                            },
                            {
                                key: "losses",
                                label: languageManager.translate(
                                    "ui_stats_losses",
                                ),
                            },
                            {
                                key: "winrate",
                                label: languageManager.translate(
                                    "ui_stats_winrate",
                                ),
                            },
                        ],
                        difficultySort,
                        onDifficultySort,
                    )}
                    <tbody>
                        {diffEntries.map(([name, row]) => (
                            <tr key={`diff-${name}`}>
                                <td>{languageManager.localize(name)}</td>
                                <td>{formatNumber(row.Victory)}</td>
                                <td>{formatNumber(row.Defeat)}</td>
                                <td>{formatPercent0(row.Winrate)}</td>
                            </tr>
                        ))}
                        <tr className={styles.statsSumRow}>
                            <td>
                                {languageManager.translate("ui_common_sum")}
                            </td>
                            <td>{formatNumber(diffTotals.wins)}</td>
                            <td>{formatNumber(diffTotals.losses)}</td>
                            <td>
                                {diffTotalGames > 0
                                    ? `${Math.round((100 * diffTotals.wins) / diffTotalGames)}%`
                                    : "-"}
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
    );
}
