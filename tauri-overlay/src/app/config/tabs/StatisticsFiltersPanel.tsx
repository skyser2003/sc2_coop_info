import * as React from "react";
import Grid from "@mui/material/Grid";
import type { LanguageManager } from "../../i18n/languageManager";
import type {
    StatisticsBoolFilterKey,
    StatisticsDifficultyKey,
    StatisticsPayload,
    StatisticsRegionKey,
    StatisticsState,
    StatsHelpers,
} from "../types";
import styles from "../configStyles";

type StatisticsFiltersPanelProps = {
    actions: StatsHelpers;
    languageManager: LanguageManager;
    statsPayload: StatisticsPayload | null;
    statsState: StatisticsState;
};

const DIFFICULTY_FILTERS: ReadonlyArray<{
    key: StatisticsDifficultyKey;
    label: string;
}> = [
    { key: "Casual", label: "Casual" },
    { key: "Normal", label: "Normal" },
    { key: "Hard", label: "Hard" },
    { key: "Brutal", label: "Brutal" },
    { key: "BrutalPlus1", label: "Brutal+1" },
    { key: "BrutalPlus2", label: "Brutal+2" },
    { key: "BrutalPlus3", label: "Brutal+3" },
    { key: "BrutalPlus4", label: "Brutal+4" },
    { key: "BrutalPlus5", label: "Brutal+5" },
    { key: "BrutalPlus6", label: "Brutal+6" },
];

const REGION_FILTERS: ReadonlyArray<{
    key: StatisticsRegionKey;
    labelId: string;
}> = [
    { key: "NA", labelId: "ui_stats_region_americas" },
    { key: "EU", labelId: "ui_stats_region_europe" },
    { key: "KR", labelId: "ui_stats_region_asia" },
    { key: "CN", labelId: "ui_stats_region_china" },
];

const BOOL_GROUPS: ReadonlyArray<{
    titleId: string;
    filters: ReadonlyArray<{ key: StatisticsBoolFilterKey; labelId: string }>;
}> = [
    {
        titleId: "ui_stats_group_game_type",
        filters: [
            { key: "includeNormalGames", labelId: "ui_stats_normal_games" },
            { key: "includeMutations", labelId: "ui_stats_mutations" },
        ],
    },
    {
        titleId: "ui_stats_group_game_result",
        filters: [
            { key: "includeWins", labelId: "ui_stats_include_wins" },
            { key: "includeLosses", labelId: "ui_stats_include_losses" },
        ],
    },
    {
        titleId: "ui_stats_group_main_level",
        filters: [
            {
                key: "includeMainSub15",
                labelId: "ui_stats_include_levels_1_14",
            },
            {
                key: "includeMainOver15",
                labelId: "ui_stats_include_levels_15_plus",
            },
        ],
    },
    {
        titleId: "ui_stats_group_main_mastery",
        filters: [
            {
                key: "includeMainNormalMastery",
                labelId: "ui_stats_include_normal_mastery_sum",
            },
            {
                key: "includeMainAbnormalMastery",
                labelId: "ui_stats_include_abnormal_mastery_sum",
            },
        ],
    },
    {
        titleId: "ui_stats_group_ally_level",
        filters: [
            {
                key: "includeAllySub15",
                labelId: "ui_stats_include_levels_1_14",
            },
            {
                key: "includeAllyOver15",
                labelId: "ui_stats_include_levels_15_plus",
            },
        ],
    },
    {
        titleId: "ui_stats_group_ally_mastery",
        filters: [
            {
                key: "includeAllyNormalMastery",
                labelId: "ui_stats_include_normal_mastery_sum",
            },
            {
                key: "includeAllyAbnormalMastery",
                labelId: "ui_stats_include_abnormal_mastery_sum",
            },
        ],
    },
    {
        titleId: "ui_stats_group_etc",
        filters: [
            {
                key: "overrideFolderSelection",
                labelId: "ui_stats_override_folder",
            },
            { key: "includeMultiBox", labelId: "ui_stats_include_multibox" },
        ],
    },
];

function formatNumber(value: number): string {
    return value.toLocaleString("en-US");
}

function translate(
    languageManager: LanguageManager,
    id: string,
    values: Record<string, string | number> = {},
): string {
    return Object.entries(values).reduce(
        (text, [key, value]) => text.split(`{{${key}}}`).join(String(value)),
        languageManager.translate(id),
    );
}

function FilterCheckbox({
    checked,
    label,
    onChange,
}: {
    checked: boolean;
    label: string;
    onChange: () => void;
}): React.ReactNode {
    return (
        <label className={styles.statsCheckboxLine}>
            <input type="checkbox" checked={checked} onChange={onChange} />
            <span>{label}</span>
        </label>
    );
}

function BoolFilterGroup({
    actions,
    filters,
    languageManager,
    statsState,
    titleId,
}: {
    actions: StatsHelpers;
    filters: ReadonlyArray<{ key: StatisticsBoolFilterKey; labelId: string }>;
    languageManager: LanguageManager;
    statsState: StatisticsState;
    titleId: string;
}): React.ReactNode {
    return (
        <div className={styles.statsFilterGroup}>
            <h4>{languageManager.translate(titleId)}</h4>
            {filters.map((filter) => (
                <FilterCheckbox
                    key={filter.key}
                    label={languageManager.translate(filter.labelId)}
                    checked={statsState.filters[filter.key]}
                    onChange={() => actions.setStatsBool(filter.key)}
                />
            ))}
        </div>
    );
}

export default function StatisticsFiltersPanel({
    actions,
    languageManager,
    statsPayload,
    statsState,
}: StatisticsFiltersPanelProps): React.ReactNode {
    const gamesFound = Number(statsPayload?.games ?? 0);
    const t = (id: string) => languageManager.translate(id);

    return (
        <Grid
            container
            spacing={2.25}
            alignItems="flex-start"
            className={styles.statsTopGrid}
        >
            <Grid size={8}>
                <div className={styles.statsCheckCols}>
                    <div className={styles.statsFilterGroup}>
                        <h4>{t("ui_stats_group_difficulty")}</h4>
                        {DIFFICULTY_FILTERS.map((filter) => (
                            <FilterCheckbox
                                key={filter.key}
                                label={languageManager.localize(filter.label)}
                                checked={
                                    statsState.filters.difficulties[filter.key]
                                }
                                onChange={() =>
                                    actions.toggleDifficulty(filter.key)
                                }
                            />
                        ))}
                    </div>
                    <div className={styles.statsFilterGroup}>
                        <h4>{t("ui_stats_group_region")}</h4>
                        {REGION_FILTERS.map((filter) => (
                            <FilterCheckbox
                                key={filter.key}
                                label={t(filter.labelId)}
                                checked={statsState.filters.regions[filter.key]}
                                onChange={() =>
                                    actions.toggleRegion(filter.key)
                                }
                            />
                        ))}
                    </div>
                    {BOOL_GROUPS.map((group, index) =>
                        index % 2 === 0 && index < BOOL_GROUPS.length - 1 ? (
                            <div
                                className={styles.statsFilterStack}
                                key={group.titleId}
                            >
                                <BoolFilterGroup
                                    actions={actions}
                                    filters={group.filters}
                                    languageManager={languageManager}
                                    statsState={statsState}
                                    titleId={group.titleId}
                                />
                                <BoolFilterGroup
                                    actions={actions}
                                    filters={BOOL_GROUPS[index + 1].filters}
                                    languageManager={languageManager}
                                    statsState={statsState}
                                    titleId={BOOL_GROUPS[index + 1].titleId}
                                />
                            </div>
                        ) : index === BOOL_GROUPS.length - 1 ? (
                            <BoolFilterGroup
                                actions={actions}
                                filters={group.filters}
                                key={group.titleId}
                                languageManager={languageManager}
                                statsState={statsState}
                                titleId={group.titleId}
                            />
                        ) : null,
                    )}
                </div>
            </Grid>
            <Grid size={4}>
                <div className={styles.statsFiltersSide}>
                    <div className={styles.statsMinmax}>
                        <h4>{t("ui_stats_game_length_minutes")}</h4>
                        <Grid container spacing={1}>
                            <Grid size={4}>
                                <span>{t("ui_common_minimum")}</span>
                            </Grid>
                            <Grid size={8}>
                                <input
                                    className={styles.input}
                                    type="number"
                                    min={0}
                                    max={1000}
                                    value={statsState.filters.minLength}
                                    onChange={(event) =>
                                        actions.setStatsNumber(
                                            "minLength",
                                            event.target.value,
                                        )
                                    }
                                />
                            </Grid>
                        </Grid>
                        <Grid container spacing={1}>
                            <Grid size={4}>
                                <span>{t("ui_common_maximum")}</span>
                            </Grid>
                            <Grid size={8}>
                                <input
                                    className={styles.input}
                                    type="number"
                                    min={0}
                                    max={1000}
                                    value={statsState.filters.maxLength}
                                    onChange={(event) =>
                                        actions.setStatsNumber(
                                            "maxLength",
                                            event.target.value,
                                        )
                                    }
                                />
                            </Grid>
                        </Grid>
                    </div>
                    <div className={styles.statsDates}>
                        <h4>{t("ui_stats_replay_date")}</h4>
                        <Grid container>
                            <Grid size={4}>
                                <span>{t("ui_common_from")}</span>
                            </Grid>
                            <Grid size={8}>
                                <input
                                    className={styles.input}
                                    type="date"
                                    value={statsState.filters.fromDate}
                                    onChange={(event) =>
                                        actions.setStatsText(
                                            "fromDate",
                                            event.target.value,
                                        )
                                    }
                                />
                            </Grid>
                        </Grid>
                        <Grid container>
                            <Grid size={4}>
                                <span>{t("ui_common_to")}</span>
                            </Grid>
                            <Grid size={8}>
                                <input
                                    className={styles.input}
                                    type="date"
                                    value={statsState.filters.toDate}
                                    onChange={(event) =>
                                        actions.setStatsText(
                                            "toDate",
                                            event.target.value,
                                        )
                                    }
                                />
                            </Grid>
                        </Grid>
                        <input
                            className={styles.input}
                            type="text"
                            value={statsState.filters.player}
                            placeholder={t("ui_stats_filter_ally_player")}
                            onChange={(event) =>
                                actions.setStatsText(
                                    "player",
                                    event.target.value,
                                )
                            }
                        />
                    </div>
                    <div className={styles.statsSideActions}>
                        <button
                            type="button"
                            className={styles.buttonNormal}
                            onClick={actions.startSimpleAnalysis}
                            disabled={
                                actions.isBusy ||
                                Boolean(statsPayload?.ready) ||
                                Boolean(statsPayload?.analysis_running)
                            }
                        >
                            {statsPayload?.analysis_running &&
                            statsPayload?.analysis_running_mode === "simple"
                                ? t("ui_stats_simple_running")
                                : t("ui_stats_run_simple_analysis")}
                        </button>
                        <button
                            type="button"
                            className={styles.buttonNormal}
                            onClick={actions.dumpData}
                            disabled={actions.isBusy || !statsPayload?.ready}
                        >
                            {t("ui_stats_dump_data")}
                        </button>
                        <button
                            type="button"
                            className={styles.buttonNormal}
                            onClick={actions.refreshStats}
                            disabled={actions.isBusy}
                        >
                            {actions.isBusy
                                ? t("ui_common_loading")
                                : t("ui_common_refresh")}
                        </button>
                        <p>
                            {translate(
                                languageManager,
                                "ui_stats_games_found",
                                {
                                    value: formatNumber(gamesFound),
                                },
                            )}
                        </p>
                    </div>
                </div>
            </Grid>
        </Grid>
    );
}
