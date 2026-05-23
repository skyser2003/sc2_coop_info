import * as React from "react";
import { Chart, registerables } from "chart.js";
import type { StatsCommanderDataRow } from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import { PreviewManager } from "../../previews/PreviewManager";
import type {
    StatisticsAnalysis,
    StatisticsState,
    StatsHelpers,
} from "../types";
import { sortRows, type SortState } from "./tableSort";
import { tableHeader } from "./statisticsTable";
import {
    asTableValue,
    formatNumber,
    formatPercent0,
    formatPercent1,
    masteryLabelsForLanguage,
    readNumber,
    translate,
} from "./statisticsViewModels";
import SelectionPreview from "./SelectionPreview";
import styles from "../page.module.css";

const MASTERY_DISTRIBUTION_GRAPH_TOP = 24;
const MASTERY_DISTRIBUTION_LABEL_OFFSET = 4;
const MASTERY_DISTRIBUTION_LINE_COLOR = "#60a5fa";

type StatsRow = StatsCommanderDataRow;
type NamedStatsRows = Array<[string, StatsRow]>;
type StatsSelectionField = "selectedAllyCommander" | "selectedMyCommander";
type MasteryDistributionBucket = {
    ratioPercent: number;
    percent: number;
};
type MasteryPrestigeDistribution = {
    key: string;
    label: string;
    buckets: MasteryDistributionBucket[];
};
type MasteryCategoryDistribution = {
    pairIndex: number;
    leftIndex: number;
    rightIndex: number;
    leftLabel: string;
    rightLabel: string;
    prestigeRows: MasteryPrestigeDistribution[];
};
type MasteryDistributionLineGraphProps = {
    category: MasteryCategoryDistribution;
    prestige: MasteryPrestigeDistribution;
    languageManager: LanguageManager;
};
type MasteryDistributionChartPoint = {
    x: number;
    y: number;
};
type MasteryDistributionChart = Chart<
    "line",
    MasteryDistributionChartPoint[],
    number
>;
let masteryChartComponentsRegistered = false;

function ensureMasteryChartComponentsRegistered(): void {
    if (masteryChartComponentsRegistered) {
        return;
    }
    Chart.register(...registerables);
    masteryChartComponentsRegistered = true;
}

function namedStatsRows(value: Record<string, StatsRow>): NamedStatsRows {
    return Object.entries(value);
}

function clampRatio(value: number): number {
    if (!Number.isFinite(value)) {
        return 0;
    }
    return Math.max(0, Math.min(1, value));
}

function masteryChoiceLabel(
    languageManager: LanguageManager,
    leftRatioPercent: number,
): string {
    if (leftRatioPercent >= 100) {
        return languageManager.translate("ui_stats_mastery_choice_1");
    }
    if (leftRatioPercent <= 0) {
        return languageManager.translate("ui_stats_mastery_choice_2");
    }
    if (leftRatioPercent === 50) {
        return languageManager.translate("ui_stats_mastery_choice_even");
    }
    if (leftRatioPercent > 50) {
        return languageManager.translate("ui_stats_mastery_choice_1_leaning");
    }
    return languageManager.translate("ui_stats_mastery_choice_2_leaning");
}

function masteryDistributionY(percent: number, maxPercent: number): number {
    if (maxPercent <= 0) {
        return 100;
    }
    return (
        100 - (percent / maxPercent) * (100 - MASTERY_DISTRIBUTION_GRAPH_TOP)
    );
}

function masteryDistributionLabelLeft(leftRatioPercent: number): string {
    return `${(100 - leftRatioPercent).toFixed(3)}%`;
}

function masteryDistributionLabelClass(leftRatioPercent: number): string {
    if (leftRatioPercent >= 100) {
        return [
            styles.masteryDistributionPointLabel,
            styles.masteryDistributionPointLabelLeft,
        ].join(" ");
    }
    if (leftRatioPercent <= 0) {
        return [
            styles.masteryDistributionPointLabel,
            styles.masteryDistributionPointLabelRight,
        ].join(" ");
    }
    return styles.masteryDistributionPointLabel;
}

function masteryDistributionVisibleLabels(
    buckets: MasteryDistributionBucket[],
): MasteryDistributionBucket[] {
    const endpointBuckets = buckets.filter(
        (bucket) =>
            bucket.percent > 0 &&
            (bucket.ratioPercent <= 0 || bucket.ratioPercent >= 100),
    );
    const representativeBucket = buckets
        .filter(
            (bucket) =>
                bucket.percent > 0 &&
                bucket.ratioPercent > 0 &&
                bucket.ratioPercent < 100,
        )
        .sort(
            (left, right) =>
                right.percent - left.percent ||
                left.ratioPercent - right.ratioPercent,
        )[0];

    if (!representativeBucket) {
        return endpointBuckets;
    }
    return [...endpointBuckets, representativeBucket];
}

function masteryDistributionDisplayBuckets(
    buckets: MasteryDistributionBucket[],
): MasteryDistributionBucket[] {
    const projected = Array.from({ length: 31 }, (_, point) => ({
        ratioPercent: (point / 30) * 100,
        percent: 0,
    }));

    for (const bucket of buckets) {
        const point = Math.max(
            0,
            Math.min(30, Math.round((bucket.ratioPercent / 100) * 30)),
        );
        projected[point].percent += bucket.percent;
    }

    return projected;
}

function masteryDistributionPointKey(
    bucket: MasteryDistributionBucket,
): string {
    return String(bucket.ratioPercent);
}

function masteryDistributionChartData(
    buckets: MasteryDistributionBucket[],
): MasteryDistributionChartPoint[] {
    return buckets
        .map((bucket) => ({
            x: 100 - bucket.ratioPercent,
            y: bucket.percent,
        }))
        .sort((left, right) => left.x - right.x);
}

function masteryDistributionYMax(maxPercent: number): number {
    if (maxPercent <= 0) {
        return 1;
    }
    return maxPercent / (1 - MASTERY_DISTRIBUTION_GRAPH_TOP / 100);
}

function masteryDistributionDataKey(
    buckets: MasteryDistributionBucket[],
): string {
    return buckets
        .map(
            (bucket) =>
                `${bucket.ratioPercent.toFixed(3)}:${bucket.percent.toFixed(6)}`,
        )
        .join("|");
}

function buildMasteryDistributionChart(
    canvas: HTMLCanvasElement,
    data: MasteryDistributionChartPoint[],
    maxPercent: number,
): MasteryDistributionChart {
    ensureMasteryChartComponentsRegistered();
    return new Chart(canvas, {
        type: "line",
        data: {
            datasets: [
                {
                    data,
                    borderColor: MASTERY_DISTRIBUTION_LINE_COLOR,
                    borderWidth: 2.3,
                    clip: false,
                    pointRadius: 0,
                    pointHoverRadius: 0,
                    tension: 0,
                },
            ],
        },
        options: {
            animation: false,
            responsive: true,
            maintainAspectRatio: false,
            resizeDelay: 0,
            events: [],
            parsing: false,
            layout: {
                padding: 0,
            },
            plugins: {
                legend: {
                    display: false,
                },
                tooltip: {
                    enabled: false,
                },
            },
            scales: {
                x: {
                    type: "linear",
                    min: 0,
                    max: 100,
                    display: false,
                },
                y: {
                    type: "linear",
                    min: 0,
                    max: masteryDistributionYMax(maxPercent),
                    display: false,
                },
            },
        },
    });
}

function masteryLabelAt(
    labels: string[],
    index: number,
    languageManager: LanguageManager,
): string {
    return languageManager.localize(
        asTableValue(
            labels[index] ||
                translate(languageManager, "ui_stats_mastery_fallback", {
                    index: index + 1,
                }),
        ),
    );
}

function buildMasteryCategoryDistributions(
    masteryDistributionByPrestige: StatsRow["MasteryDistributionByPrestige"],
    masteryLabels: string[],
    languageManager: LanguageManager,
): MasteryCategoryDistribution[] {
    const categories: MasteryCategoryDistribution[] = [];
    const prestigeKeys = ["0", "1", "2", "3"];

    for (let pairIndex = 0; pairIndex < 3; pairIndex += 1) {
        const leftIndex = pairIndex * 2;
        const rightIndex = leftIndex + 1;
        const prestigeRows = prestigeKeys.map((prestigeKey) => {
            const prestigeDistribution =
                masteryDistributionByPrestige[prestigeKey] ?? {};
            const pairDistribution =
                prestigeDistribution[String(pairIndex)] ?? {};
            const buckets: MasteryDistributionBucket[] = Object.entries(
                pairDistribution,
            )
                .map(([ratioPercent, percent]) => ({
                    ratioPercent: Math.max(
                        0,
                        Math.min(100, Number(ratioPercent)),
                    ),
                    percent: clampRatio(percent),
                }))
                .filter((bucket) => Number.isFinite(bucket.ratioPercent))
                .sort((left, right) => left.ratioPercent - right.ratioPercent);
            return {
                key: prestigeKey,
                label: `${languageManager.translate(
                    "ui_stats_prestige_label",
                )} ${prestigeKey}`,
                buckets,
            };
        });
        const hasData = prestigeRows.some((prestige) =>
            prestige.buckets.some((bucket) => bucket.percent > 0),
        );

        if (!hasData) {
            continue;
        }

        categories.push({
            pairIndex,
            leftIndex,
            rightIndex,
            leftLabel: masteryLabelAt(
                masteryLabels,
                leftIndex,
                languageManager,
            ),
            rightLabel: masteryLabelAt(
                masteryLabels,
                rightIndex,
                languageManager,
            ),
            prestigeRows,
        });
    }

    return categories;
}

function MasteryDistributionLineGraph({
    category,
    prestige,
    languageManager,
}: MasteryDistributionLineGraphProps) {
    const canvasRef = React.useRef<HTMLCanvasElement | null>(null);
    const displayBuckets = masteryDistributionDisplayBuckets(prestige.buckets);
    const maxPercent = displayBuckets.reduce(
        (current, bucket) => Math.max(current, bucket.percent),
        0,
    );
    const dataKey = masteryDistributionDataKey(displayBuckets);
    const labelY = (bucket: MasteryDistributionBucket): number =>
        masteryDistributionY(bucket.percent, maxPercent);
    const labelTop = (bucket: MasteryDistributionBucket): string =>
        `${Math.max(
            8,
            Math.min(92, labelY(bucket) - MASTERY_DISTRIBUTION_LABEL_OFFSET),
        ).toFixed(3)}%`;
    const visibleLabelBuckets =
        masteryDistributionVisibleLabels(displayBuckets);
    const visibleLabelKeys = new Set(
        visibleLabelBuckets.map(masteryDistributionPointKey),
    );

    React.useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas) {
            return;
        }

        const chart = buildMasteryDistributionChart(
            canvas,
            masteryDistributionChartData(displayBuckets),
            maxPercent,
        );

        return () => {
            chart.destroy();
        };
    }, [dataKey, maxPercent]);

    return (
        <div
            className={styles.masteryDistributionLineGraph}
            data-testid="mastery-distribution-line"
            role="img"
            aria-label={translate(
                languageManager,
                "ui_stats_mastery_distribution_graph_label",
                {
                    prestige: prestige.label,
                    mastery: category.leftLabel,
                },
            )}
        >
            <div
                className={styles.masteryDistributionChartSurface}
                aria-hidden="true"
            >
                <canvas
                    className={styles.masteryDistributionChartCanvas}
                    ref={canvasRef}
                />
            </div>
            <div className={styles.masteryDistributionOverlaySurface}>
                <span
                    className={styles.masteryDistributionGridLine}
                    style={{ top: "100%" }}
                />
                <span
                    className={styles.masteryDistributionGridLine}
                    style={{
                        top: `${(100 + MASTERY_DISTRIBUTION_GRAPH_TOP) / 2}%`,
                    }}
                />
                <span
                    className={styles.masteryDistributionEvenLine}
                    data-testid="mastery-distribution-even-line"
                />
                {displayBuckets
                    .filter(
                        (bucket) =>
                            bucket.percent > 0 &&
                            visibleLabelKeys.has(
                                masteryDistributionPointKey(bucket),
                            ),
                    )
                    .map((bucket) => (
                        <span
                            className={styles.masteryDistributionPoint}
                            data-testid="mastery-distribution-point"
                            role="img"
                            key={`mastery-${category.pairIndex}-${prestige.key}-${bucket.ratioPercent}`}
                            style={{
                                left: masteryDistributionLabelLeft(
                                    bucket.ratioPercent,
                                ),
                                top: `${labelY(bucket).toFixed(3)}%`,
                            }}
                            aria-label={`${prestige.label} ${masteryChoiceLabel(languageManager, bucket.ratioPercent)}: ${formatPercent1(bucket.percent)}`}
                        />
                    ))}
                {visibleLabelBuckets.map((bucket) => (
                    <span
                        className={masteryDistributionLabelClass(
                            bucket.ratioPercent,
                        )}
                        data-testid="mastery-distribution-point-label"
                        key={`mastery-label-${category.pairIndex}-${prestige.key}-${bucket.ratioPercent}`}
                        style={{
                            left: masteryDistributionLabelLeft(
                                bucket.ratioPercent,
                            ),
                            top: labelTop(bucket),
                        }}
                    >
                        {formatPercent1(bucket.percent)}
                    </span>
                ))}
            </div>
        </div>
    );
}

function renderCommanderDetails(
    commander: string | null,
    entry: StatsRow | null,
    languageManager: LanguageManager,
    previewManager: PreviewManager,
) {
    if (!commander || !entry) {
        return (
            <div className={styles.statsDetailEmpty}>
                {languageManager.translate("ui_stats_select_commander")}
            </div>
        );
    }

    const displayCommander = languageManager.localize(commander);
    const commanderPreview = previewManager.commander(commander);

    const commanderKey = languageManager.englishLabel(commander);

    const masteryLabels = masteryLabelsForLanguage(
        languageManager,
        commanderKey,
    );
    const prestigeSelectionKeys = ["0", "1", "2", "3"];
    const prestigeSelection = entry.Prestige;
    const masteryCategories = buildMasteryCategoryDistributions(
        entry.MasteryDistributionByPrestige,
        masteryLabels,
        languageManager,
    );

    return (
        <div className={styles.statsCommanderDetail}>
            <div className={styles.statsCommanderTop}>
                <div className={styles.statsCommanderSummary}>
                    <div className={styles.statsCommanderMeta}>
                        <span>
                            {`${languageManager.translate("ui_stats_frequency")}: `}
                            <strong>{formatPercent1(entry.Frequency)}</strong>
                        </span>
                        <span>
                            {`${languageManager.translate("ui_players_column_apm")} ${languageManager.translate("ui_stats_avg")}: `}
                            <strong>
                                {Math.round(Number(entry.MedianAPM || 0))}
                            </strong>
                        </span>
                    </div>
                    <h4 className={styles.statsCommanderSubheading}>
                        {languageManager.translate("ui_stats_mastery")} /{" "}
                        {languageManager.translate("ui_stats_prestige_label")}{" "}
                        {languageManager.translate(
                            "ui_stats_statistics_postfix",
                        )}
                    </h4>
                    <div className={styles.tableWrap}>
                        <table
                            className={[
                                styles.dataTable,
                                styles.statsDense,
                                styles.statsCommanderTable,
                            ]
                                .filter(Boolean)
                                .join(" ")}
                        >
                            <thead>
                                <tr>
                                    <th>
                                        {languageManager.translate(
                                            "ui_stats_prestige_label",
                                        )}{" "}
                                        0
                                    </th>
                                    <th>
                                        {languageManager.translate(
                                            "ui_stats_prestige_label",
                                        )}{" "}
                                        1
                                    </th>
                                    <th>
                                        {languageManager.translate(
                                            "ui_stats_prestige_label",
                                        )}{" "}
                                        2
                                    </th>
                                    <th>
                                        {languageManager.translate(
                                            "ui_stats_prestige_label",
                                        )}{" "}
                                        3
                                    </th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr
                                    className={styles.statsCommanderPrestigeRow}
                                >
                                    {prestigeSelectionKeys.map(
                                        (prestigeKey) => (
                                            <td
                                                className={
                                                    styles.statsCommanderTablePct
                                                }
                                                key={`preset-${prestigeKey}`}
                                            >
                                                {formatPercent0(
                                                    prestigeSelection[
                                                        prestigeKey
                                                    ] || 0,
                                                )}
                                            </td>
                                        ),
                                    )}
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </div>
                <SelectionPreview
                    assetUrl={commanderPreview.url}
                    title={displayCommander}
                    kind="commander"
                    className={styles.statsCommanderHero}
                    titleClassName={styles.statsCommanderTitle}
                />
            </div>
            <div className={styles.statsCommanderBottom}>
                <div className={styles.statsCommanderBottomCol}>
                    <div
                        className={styles.masteryDistributionList}
                        data-testid="mastery-distribution-list"
                    >
                        {masteryCategories.length === 0 ? (
                            <div className={styles.statsDetailEmpty}>
                                {languageManager.translate(
                                    "ui_stats_no_mastery_data",
                                )}
                            </div>
                        ) : (
                            masteryCategories.map((category) => (
                                <section
                                    className={
                                        styles.masteryDistributionCategory
                                    }
                                    key={`mastery-category-${category.pairIndex}`}
                                    data-testid="mastery-distribution-category"
                                >
                                    <div
                                        className={
                                            styles.masteryDistributionHeader
                                        }
                                        data-testid="mastery-distribution-header"
                                    >
                                        <strong>
                                            {`${languageManager.translate(
                                                "ui_stats_mastery",
                                            )} ${category.pairIndex + 1}`}
                                        </strong>
                                        <span>
                                            {`${languageManager.translate(
                                                "ui_stats_mastery_choice_1",
                                            )} - ${category.leftLabel}`}
                                        </span>
                                        <span>
                                            {`${languageManager.translate(
                                                "ui_stats_mastery_choice_2",
                                            )} - ${category.rightLabel}`}
                                        </span>
                                    </div>
                                    <div
                                        className={
                                            styles.masteryDistributionPrestigeList
                                        }
                                        data-testid="mastery-distribution-prestige-list"
                                    >
                                        {category.prestigeRows.map(
                                            (prestige) => (
                                                <div
                                                    className={
                                                        styles.masteryDistributionPrestigePanel
                                                    }
                                                    key={`mastery-category-${category.pairIndex}-${prestige.key}`}
                                                >
                                                    <h5>{prestige.label}</h5>
                                                    <MasteryDistributionLineGraph
                                                        category={category}
                                                        prestige={prestige}
                                                        languageManager={
                                                            languageManager
                                                        }
                                                    />
                                                    <div
                                                        className={
                                                            styles.masteryDistributionAxis
                                                        }
                                                    >
                                                        <span>
                                                            {languageManager.translate(
                                                                "ui_stats_mastery_choice_1",
                                                            )}
                                                        </span>
                                                        <span>
                                                            {languageManager.translate(
                                                                "ui_stats_mastery_choice_even",
                                                            )}
                                                        </span>
                                                        <span>
                                                            {languageManager.translate(
                                                                "ui_stats_mastery_choice_2",
                                                            )}
                                                        </span>
                                                    </div>
                                                </div>
                                            ),
                                        )}
                                    </div>
                                </section>
                            ))
                        )}
                    </div>
                </div>
            </div>
        </div>
    );
}

function renderStatsCommanders(
    analysis: StatisticsAnalysis,
    statsState: StatisticsState,
    actions: StatsHelpers,
    allied: boolean,
    commanderSort: SortState,
    onCommanderSort: (key: string) => void,
    languageManager: LanguageManager,
    previewManager: PreviewManager,
) {
    const key = allied ? "AllyCommanderData" : "CommanderData";
    const entries = allied
        ? analysis.AllyCommanderData
        : analysis.CommanderData;
    const rowsBase = namedStatsRows(entries)
        .filter(([name]) => name !== "any")
        .sort((a, b) => a[0].localeCompare(b[0]));
    const rows = sortRows(rowsBase, commanderSort, ([name, row], sortKey) => {
        if (sortKey === "name") return languageManager.localize(name);
        if (sortKey === "freq") return readNumber(row.Frequency);
        if (sortKey === "wins") return readNumber(row.Victory);
        if (sortKey === "losses") return readNumber(row.Defeat);
        if (sortKey === "win") return readNumber(row.Winrate);
        if (sortKey === "apm") return readNumber(row.MedianAPM);
        if (sortKey === "kills") return readNumber(row.KillFraction);
        return "";
    });
    const selectedField: StatsSelectionField = allied
        ? "selectedAllyCommander"
        : "selectedMyCommander";
    const selectedCommander = (rows.find(
        ([name]) => name === statsState[selectedField],
    ) ||
        rows[0] || [null])[0];
    const selectedEntry = selectedCommander ? entries[selectedCommander] : null;
    const sum = entries.any ?? null;

    return (
        <div
            className={[styles.statsSubContent, styles.statsCommandersSplit]
                .filter(Boolean)
                .join(" ")}
        >
            <div
                className={[styles.statsPane, styles.statsPaneLeft]
                    .filter(Boolean)
                    .join(" ")}
            >
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
                                    label: allied
                                        ? languageManager.translate(
                                              "ui_stats_allied_commander",
                                          )
                                        : languageManager.translate(
                                              "ui_stats_commander",
                                          ),
                                },
                                {
                                    key: "freq",
                                    label: languageManager.translate(
                                        "ui_stats_freq",
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
                                    key: "win",
                                    label: languageManager.translate(
                                        "ui_stats_win_percent",
                                    ),
                                },
                                {
                                    key: "apm",
                                    label: languageManager.translate(
                                        "ui_players_column_apm",
                                    ),
                                },
                                {
                                    key: "kills",
                                    label: languageManager.translate(
                                        "ui_stats_kills",
                                    ),
                                },
                            ],
                            commanderSort,
                            onCommanderSort,
                        )}
                        <tbody>
                            {rows.map(([name, row]) => (
                                <tr
                                    key={`${key}-${name}`}
                                    className={
                                        name === selectedCommander
                                            ? styles.selectedRow
                                            : ""
                                    }
                                    onClick={() =>
                                        actions.setStatsState((current) => ({
                                            ...current,
                                            [selectedField]: name,
                                        }))
                                    }
                                >
                                    <td>{languageManager.localize(name)}</td>
                                    <td>{formatPercent1(row.Frequency)}</td>
                                    <td>{formatNumber(row.Victory || 0)}</td>
                                    <td>{formatNumber(row.Defeat || 0)}</td>
                                    <td>{formatPercent0(row.Winrate)}</td>
                                    <td>
                                        {Math.round(Number(row.MedianAPM || 0))}
                                    </td>
                                    <td>
                                        {row.detailedCount === 0
                                            ? "-"
                                            : formatPercent0(
                                                  row.KillFraction || 0,
                                              )}
                                    </td>
                                </tr>
                            ))}
                            {sum ? (
                                <tr className={styles.statsSumRow}>
                                    <td>
                                        {languageManager.translate(
                                            "ui_common_sum",
                                        )}
                                    </td>
                                    <td>{formatPercent0(sum.Frequency)}</td>
                                    <td>{formatNumber(sum.Victory || 0)}</td>
                                    <td>{formatNumber(sum.Defeat || 0)}</td>
                                    <td>{formatPercent0(sum.Winrate)}</td>
                                    <td>
                                        {Math.round(Number(sum.MedianAPM || 0))}
                                    </td>
                                    <td>
                                        {sum.detailedCount === 0
                                            ? "-"
                                            : formatPercent0(
                                                  sum.KillFraction || 0,
                                              )}
                                    </td>
                                </tr>
                            ) : null}
                        </tbody>
                    </table>
                </div>
                {allied ? (
                    <p
                        className={[styles.note, styles.statsRightNote]
                            .filter(Boolean)
                            .join(" ")}
                    >
                        {languageManager.translate(
                            "ui_stats_frequency_corrected_note",
                        )}
                    </p>
                ) : null}
            </div>
            <div
                className={[
                    styles.statsPane,
                    styles.statsPaneRight,
                    styles.statsCommanderPane,
                ]
                    .filter(Boolean)
                    .join(" ")}
            >
                {renderCommanderDetails(
                    selectedCommander,
                    selectedEntry,
                    languageManager,
                    previewManager,
                )}
            </div>
        </div>
    );
}

type StatisticsCommandersPanelProps = {
    analysis: StatisticsAnalysis;
    statsState: StatisticsState;
    actions: StatsHelpers;
    allied: boolean;
    commanderSort: SortState;
    onCommanderSort: (key: string) => void;
    languageManager: LanguageManager;
    previewManager: PreviewManager;
};

export default function StatisticsCommandersPanel({
    analysis,
    statsState,
    actions,
    allied,
    commanderSort,
    onCommanderSort,
    languageManager,
    previewManager,
}: StatisticsCommandersPanelProps): React.ReactNode {
    return renderStatsCommanders(
        analysis,
        statsState,
        actions,
        allied,
        commanderSort,
        onCommanderSort,
        languageManager,
        previewManager,
    );
}
