import * as React from "react";
import type { LanguageManager } from "../../i18n/languageManager";
import { PreviewManager } from "../../previews/PreviewManager";
import type {
    LocalizedMasteryNames,
    PrestigeNameMap,
    DisplayValue,
    JsonArray,
    JsonObject,
    JsonValue,
    StatisticsAnalysis,
    StatisticsPayload,
    StatisticsState,
    StatsHelpers,
} from "../types";
import {
    nextSortState,
    sortIndicator,
    sortRows,
    type SortState,
} from "./tableSort";
import SelectionPreview from "./SelectionPreview";

const STATS_SUBTABS = [
    { id: "maps", titleId: "ui_statistics_subtab_maps" },
    { id: "ally", titleId: "ui_statistics_subtab_allied_commanders" },
    { id: "my", titleId: "ui_statistics_subtab_my_commanders" },
    { id: "diffregion", titleId: "ui_statistics_subtab_difficulty_regions" },
    { id: "units", titleId: "ui_statistics_subtab_unit_stats" },
    { id: "amon", titleId: "ui_statistics_subtab_amon_stats" },
] as const;

const DIFFICULTY_ORDER = [
    "Casual",
    "Normal",
    "Hard",
    "Brutal",
    "B+1",
    "B+2",
    "B+3",
    "B+4",
    "B+5",
    "B+6",
];

type StatisticsTabProps = {
    statsPayload: StatisticsPayload | null;
    statsState: StatisticsState;
    actions: StatsHelpers;
    languageManager: LanguageManager;
};

type CommanderMasteryLookup = Record<string, LocalizedMasteryNames>;
type PrestigeNameLookup = PrestigeNameMap;

type FastestMapPlayer = {
    name: string;
    handle: string;
    commander: string;
    apm: number;
    masteryLevel: number;
    masteries: number[];
    prestige: number;
    prestigeName: string;
};

type FastestMapDetails = {
    length: number;
    file: string;
    date: number;
    difficulty: string;
    enemyRace: string;
    players: FastestMapPlayer[];
};

type StatsRow = JsonObject;
type NamedStatsRows = Array<[string, StatsRow]>;
type StatsSelectionField = "selectedAllyCommander" | "selectedMyCommander";
type UnitStatRow = StatsRow & {
    created?: number | string;
    made?: number | string;
    lost?: number | string;
    lost_percent?: number | string | null;
    kills?: number | string;
    KD?: number | string | null;
    kill_percentage?: number | string;
};
type UnitCommanderStats = Record<string, UnitStatRow | number | string> & {
    count?: number | string;
};
type UnitSideData = Record<string, UnitCommanderStats>;
type UnitData = {
    main?: UnitSideData;
    ally?: UnitSideData;
    amon?: Record<string, UnitStatRow>;
};

function isRecord(value: JsonValue | undefined): value is JsonObject {
    return value !== null && typeof value === "object" && !Array.isArray(value);
}

function asStatsRow(value: JsonValue | undefined): StatsRow {
    return isRecord(value) ? value : {};
}

function namedStatsRows(value: JsonValue | undefined): NamedStatsRows {
    if (!isRecord(value)) {
        return [];
    }

    return Object.entries(value).map(([name, row]) => [name, asStatsRow(row)]);
}

function readNumber(value: DisplayValue, fallback: number = 0): number {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : fallback;
}

function readStringArray(value: JsonValue | undefined): string[] {
    if (!Array.isArray(value)) {
        return [];
    }

    return value.filter((item): item is string => typeof item === "string");
}

function readNumberArray(value: JsonValue | undefined): number[] {
    if (!Array.isArray(value)) {
        return [];
    }

    return value.map((item) => Number(item)).filter(Number.isFinite);
}

function readCommanderMasteryLookup(
    value: StatisticsPayload["commander_mastery"],
): CommanderMasteryLookup {
    if (!isRecord(value)) {
        return {};
    }

    const entries = Object.entries(value).map(([commander, labels]) => {
        if (Array.isArray(labels)) {
            const english = readStringArray(labels);
            return [commander, { en: english, ko: [] }] as const;
        }

        if (!isRecord(labels)) {
            return [commander, { en: [], ko: [] }] as const;
        }

        return [
            commander,
            {
                en: readStringArray(labels.en),
                ko: readStringArray(labels.ko),
            },
        ] as const;
    });
    return Object.fromEntries(entries);
}

function masteryLabelsForLanguage(
    commanderMastery: CommanderMasteryLookup,
    commander: string,
    language: "en" | "ko",
): string[] {
    const localized = commanderMastery[commander];
    if (!localized) {
        return [];
    }

    return localized[language].length > 0 ? localized[language] : localized.en;
}

function readPrestigeNameLookup(
    value: StatisticsPayload["prestige_names"],
): PrestigeNameLookup {
    if (!isRecord(value)) {
        return {};
    }

    const entries = Object.entries(value).map(([commander, labels]) => {
        if (!isRecord(labels)) {
            return [commander, { en: [], ko: [] }] as const;
        }

        return [
            commander,
            {
                en: readStringArray(labels.en),
                ko: readStringArray(labels.ko),
            },
        ] as const;
    });
    return Object.fromEntries(entries);
}

function prestigeLabelForLanguage(
    prestigeNames: PrestigeNameLookup,
    commander: string,
    prestige: number,
    language: "en" | "ko",
): string {
    const localized = prestigeNames[commander];
    if (!localized) {
        return `P${prestige}`;
    }

    return (
        localized[language]?.[prestige] ||
        localized.en?.[prestige] ||
        `P${prestige}`
    );
}

function readFastestMapPlayer(
    value: JsonValue | undefined,
): FastestMapPlayer | null {
    if (!isRecord(value)) {
        return null;
    }

    return {
        name: asTableValue(value.name),
        handle: asTableValue(value.handle),
        commander: asTableValue(value.commander),
        apm: Number(value.apm || 0),
        masteryLevel: Number(value.mastery_level || 0),
        masteries: readNumberArray(value.masteries),
        prestige: Number(value.prestige || 0),
        prestigeName: asTableValue(value.prestige_name),
    };
}

function readFastestMapDetails(
    value: JsonValue | undefined,
): FastestMapDetails {
    if (!isRecord(value)) {
        return {
            length: 0,
            file: "",
            date: 0,
            difficulty: "",
            enemyRace: "",
            players: [],
        };
    }

    const playersSource = Array.isArray(value.players) ? value.players : [];
    const players = playersSource
        .map((player) => readFastestMapPlayer(player))
        .filter((player): player is FastestMapPlayer => player !== null);

    return {
        length: Number(value.length || 0),
        file: asTableValue(value.file),
        date: Number(value.date || 0),
        difficulty: asTableValue(value.difficulty),
        enemyRace: asTableValue(value.enemy_race),
        players,
    };
}

function normalizeHandleKey(value: DisplayValue): string {
    const text = asTableValue(value).trim().toLowerCase();
    return text.includes("-s2-") ? text : "";
}

function asTableValue(value: DisplayValue) {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

function formatPercent(value: DisplayValue) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "0.0%";
    }
    return `${(num * 100).toFixed(1)}%`;
}

function formatPercent0(value: DisplayValue) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "-";
    }
    return `${(num * 100).toFixed(0)}%`;
}

function formatPercent1(value: DisplayValue) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "-";
    }
    return `${(num * 100).toFixed(1)}%`;
}

function formatNumber(value: DisplayValue) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return asTableValue(value);
    }
    return num.toLocaleString("en-US");
}

function formatDurationSeconds(value: DisplayValue) {
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

function formatReplayTime(value: DisplayValue) {
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

function orderedDifficultyEntries(
    diffData: JsonValue | undefined,
): NamedStatsRows {
    const rows: NamedStatsRows = [];
    const difficultyRows = asStatsRow(diffData);
    const existing = Object.keys(difficultyRows);
    const seen = new Set();
    for (const name of DIFFICULTY_ORDER) {
        if (difficultyRows[name]) {
            seen.add(name);
            rows.push([name, asStatsRow(difficultyRows[name])]);
        }
    }

    for (const name of existing) {
        if (!seen.has(name)) {
            if (name === "B+" || name.toLowerCase().startsWith("brutal+")) {
                rows.push([name, asStatsRow(difficultyRows[name])]);
                continue;
            }
            if (/^B\+\d+$/.test(name)) {
                rows.push([name, asStatsRow(difficultyRows[name])]);
            }
        }
    }

    rows.sort((left, right) => {
        const leftOrder = DIFFICULTY_ORDER.indexOf(left[0]);
        const rightOrder = DIFFICULTY_ORDER.indexOf(right[0]);
        if (leftOrder !== -1 || rightOrder !== -1) {
            if (leftOrder === -1) return 1;
            if (rightOrder === -1) return -1;
            return leftOrder - rightOrder;
        }
        return left[0].localeCompare(right[0]);
    });
    return rows;
}

function difficultySortRank(
    name: string,
    languageManager: LanguageManager,
): number {
    const id = languageManager.idFromValue(name);
    const normalized = (() => {
        switch (id) {
            case "difficulty_casual":
                return "Casual";
            case "difficulty_normal":
                return "Normal";
            case "difficulty_hard":
                return "Hard";
            case "difficulty_brutal":
                return "Brutal";
            case "difficulty_brutal_plus_1":
                return "B+1";
            case "difficulty_brutal_plus_2":
                return "B+2";
            case "difficulty_brutal_plus_3":
                return "B+3";
            case "difficulty_brutal_plus_4":
                return "B+4";
            case "difficulty_brutal_plus_5":
                return "B+5";
            case "difficulty_brutal_plus_6":
                return "B+6";
            default:
                return name;
        }
    })();
    const knownIndex = DIFFICULTY_ORDER.indexOf(normalized);
    if (knownIndex !== -1) {
        return knownIndex;
    }
    return DIFFICULTY_ORDER.length;
}

type HeaderColumn = {
    key: string;
    label: string;
    className?: string;
};

function tableHeader(
    columns: HeaderColumn[],
    sortState: SortState = null,
    onSort: ((key: string) => void) | null = null,
) {
    return (
        <thead>
            <tr>
                {columns.map((column) => (
                    <th key={column.key} className={column.className || ""}>
                        {onSort ? (
                            <button
                                type="button"
                                className="table-sort-btn"
                                onClick={() => onSort(column.key)}
                            >
                                {`${column.label}${sortIndicator(sortState, column.key)}`}
                            </button>
                        ) : (
                            column.label
                        )}
                    </th>
                ))}
            </tr>
        </thead>
    );
}

function renderCommanderDetails(
    commander: string | null,
    entry: StatsRow | null,
    statsPayload: StatisticsPayload | null,
    languageManager: LanguageManager,
    previewManager: PreviewManager,
) {
    if (!commander || !entry) {
        return (
            <div className="stats-detail-empty">
                {languageManager.translate("ui_stats_select_commander")}
            </div>
        );
    }

    const displayCommander = languageManager.localize(commander);
    const commanderPreview = previewManager.commander(commander);
    const commanderMastery = readCommanderMasteryLookup(
        statsPayload.commander_mastery,
    );
    const masteryLabels = masteryLabelsForLanguage(
        commanderMastery,
        commander,
        languageManager.currentLanguage(),
    );
    const mastery = entry.Mastery || {};
    const masteryKeys = Object.keys(mastery)
        .map((key) => Number(key))
        .filter((key) => Number.isFinite(key))
        .sort((a, b) => a - b);
    const masteryByPrestige = entry.MasteryByPrestige || {};
    const masteryByPrestigeKeys = ["0", "1", "2", "3"];
    const prestigeSelection = entry.Prestige || {};
    const prestigeSelectionTotal = Object.values(
        prestigeSelection,
    ).reduce<number>((sum, value) => sum + Number(value), 0);

    const getMasteryPercent = (masteryIdx: number, prestigeKey: string) => {
        const byPrestige = masteryByPrestige[prestigeKey] || {};
        return Number(byPrestige[masteryIdx] || 0);
    };
    const getMasteryTotalPercent = (masteryIdx: number) => {
        return Number(mastery[masteryIdx] || 0);
    };

    return (
        <div className="stats-commander-detail">
            <SelectionPreview
                assetUrl={commanderPreview.url}
                title={displayCommander}
                kind="commander"
                className="stats-commander-hero"
                titleClassName="stats-commander-title"
            />
            <div className="stats-commander-meta">
                <span>
                    {`${languageManager.translate("ui_stats_frequency")}: `}
                    <strong>{formatPercent1(entry.Frequency)}</strong>
                </span>
                <span>
                    {`${languageManager.translate("ui_players_column_apm")} ${languageManager.translate("ui_stats_avg")}: `}
                    <strong>{Math.round(Number(entry.MedianAPM || 0))}</strong>
                </span>
            </div>
            <div className="stats-commander-bottom">
                <div className="stats-commander-bottom-col">
                    <h4 className="stats-commander-subheading">
                        {languageManager.translate("ui_stats_mastery")} /{" "}
                        {languageManager.translate("ui_stats_prestige_label")}{" "}
                        {languageManager.translate(
                            "ui_stats_statistics_postfix",
                        )}
                    </h4>
                    <div className="table-wrap">
                        <table className="data-table stats-dense stats-commander-table">
                            <thead>
                                <tr>
                                    <th>
                                        {languageManager.translate(
                                            "ui_stats_mastery",
                                        )}
                                    </th>
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
                                    <th>
                                        {languageManager.translate(
                                            "ui_common_total",
                                        )}
                                    </th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr className="stats-commander-prestige-row">
                                    <td>
                                        {languageManager.translate(
                                            "ui_stats_prestige_selection",
                                        )}
                                    </td>
                                    {masteryByPrestigeKeys.map(
                                        (prestigeKey) => (
                                            <td
                                                className="stats-commander-table-pct"
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
                                    <td className="stats-commander-table-pct">
                                        {formatPercent0(prestigeSelectionTotal)}
                                    </td>
                                </tr>
                                <tr className="stats-commander-empty-row">
                                    <td
                                        colSpan={6}
                                        className="stats-commander-empty-row-cell"
                                    >
                                        {" "}
                                    </td>
                                </tr>
                                {masteryKeys.length === 0 ? (
                                    <tr>
                                        <td colSpan={6} className="empty-cell">
                                            {languageManager.translate(
                                                "ui_stats_no_mastery_data",
                                            )}
                                        </td>
                                    </tr>
                                ) : (
                                    masteryKeys.map((idx) => (
                                        <tr
                                            key={`m-${idx}`}
                                            className={`stats-commander-mastery-row ${
                                                idx === 2 || idx === 4
                                                    ? "stats-commander-category-gap"
                                                    : ""
                                            }`}
                                        >
                                            <td>
                                                {languageManager.localize(
                                                    asTableValue(
                                                        masteryLabels[idx] ||
                                                            translate(
                                                                languageManager,
                                                                "ui_stats_mastery_fallback",
                                                                {
                                                                    index:
                                                                        idx + 1,
                                                                },
                                                            ),
                                                    ),
                                                )}
                                            </td>
                                            {masteryByPrestigeKeys.map(
                                                (prestigeKey) => (
                                                    <td
                                                        className="stats-commander-table-pct"
                                                        key={`m-${idx}-${prestigeKey}`}
                                                    >
                                                        {formatPercent0(
                                                            getMasteryPercent(
                                                                idx,
                                                                prestigeKey,
                                                            ),
                                                        )}
                                                    </td>
                                                ),
                                            )}
                                            <td className="stats-commander-table-pct">
                                                {formatPercent0(
                                                    getMasteryTotalPercent(idx),
                                                )}
                                            </td>
                                        </tr>
                                    ))
                                )}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </div>
    );
}

function fastestMapPrestigeLabel(
    player: FastestMapPlayer,
    prestigeNames: PrestigeNameLookup,
    languageManager: LanguageManager,
): string {
    const prestige = Math.max(0, Math.round(player.prestige));
    const prestigeIndex = `P${prestige}`;
    const localizedLabel = prestigeLabelForLanguage(
        prestigeNames,
        player.commander,
        prestige,
        languageManager.currentLanguage(),
    );
    if (localizedLabel !== prestigeIndex) {
        return `${localizedLabel} (${prestigeIndex})`;
    }

    if (player.prestigeName) {
        return `${languageManager.localize(player.prestigeName)} (${prestigeIndex})`;
    }

    return prestigeIndex;
}

function fastestMapMasteryRows(
    player: FastestMapPlayer,
    commanderMastery: CommanderMasteryLookup,
    languageManager: LanguageManager,
): string[] {
    const labels = masteryLabelsForLanguage(
        commanderMastery,
        player.commander,
        languageManager.currentLanguage(),
    );
    const values = player.masteries;
    if (values.length === 0 && labels.length === 0) {
        return [];
    }

    const rows: string[] = [];
    for (let pairIndex = 0; pairIndex < 3; pairIndex += 1) {
        const leftIndex = pairIndex * 2;
        const rightIndex = leftIndex + 1;
        const leftLabel = languageManager.localize(
            labels[leftIndex] ||
                translate(languageManager, "ui_stats_mastery_fallback", {
                    index: leftIndex + 1,
                }),
        );
        const rightLabel = languageManager.localize(
            labels[rightIndex] ||
                translate(languageManager, "ui_stats_mastery_fallback", {
                    index: rightIndex + 1,
                }),
        );
        const leftValue = Math.round(Number(values[leftIndex] || 0));
        const rightValue = Math.round(Number(values[rightIndex] || 0));

        rows.push(`${leftValue} ${leftLabel}`);
        rows.push(`${rightValue} ${rightLabel}`);
    }
    return rows;
}

function renderFastestMapPlayer(
    player: FastestMapPlayer,
    commanderMastery: CommanderMasteryLookup,
    prestigeNames: PrestigeNameLookup,
    key: string,
    languageManager: LanguageManager,
) {
    const masteryRows = fastestMapMasteryRows(
        player,
        commanderMastery,
        languageManager,
    );
    const masteryLevel =
        player.masteryLevel > 0
            ? `Lv. ${Math.round(player.masteryLevel)}`
            : "-";

    return (
        <div className="stats-map-player" key={key}>
            <div className="stats-map-player-head">
                <h4>{asTableValue(player.name)}</h4>
                <span className="stats-map-player-apm">
                    {`${Math.round(player.apm || 0)} APM`}
                </span>
            </div>
            <p className="stats-map-player-line">
                <strong>
                    {languageManager.translate("ui_stats_commander_label")}
                </strong>
                <span>{languageManager.localize(player.commander) || "-"}</span>
            </p>
            <p className="stats-map-player-line">
                <strong>
                    {languageManager.translate("ui_stats_prestige_label")}
                </strong>
                <span>
                    {fastestMapPrestigeLabel(
                        player,
                        prestigeNames,
                        languageManager,
                    )}
                </span>
            </p>
            <p className="stats-map-player-line">
                <strong>
                    {languageManager.translate("ui_stats_mastery_level")}
                </strong>
                <span>{masteryLevel}</span>
            </p>
            <div className="stats-map-masteries">
                <strong>
                    {languageManager.translate("ui_stats_masteries")}
                </strong>
                {masteryRows.length === 0 ? (
                    <span className="stats-map-player-empty">
                        {languageManager.translate("ui_stats_no_mastery_data")}
                    </span>
                ) : (
                    masteryRows.map((row, index) => (
                        <span
                            className="stats-map-mastery-row"
                            key={`${key}-mastery-${index}`}
                        >
                            {row}
                        </span>
                    ))
                )}
            </div>
        </div>
    );
}

function renderStatsMaps(
    analysis: StatisticsAnalysis,
    statsState: StatisticsState,
    actions: StatsHelpers,
    statsPayload: StatisticsPayload | null,
    mapSort: SortState,
    onMapSort: (key: string) => void,
    languageManager: LanguageManager,
    previewManager: PreviewManager,
) {
    const mapData = asStatsRow(analysis && analysis.MapData);
    const mapEntriesBase = namedStatsRows(mapData).sort((a, b) =>
        a[0].localeCompare(b[0]),
    );
    const mapEntries = sortRows(mapEntriesBase, mapSort, ([name, row], key) => {
        if (key === "name") return languageManager.localize(name);
        if (key === "avg") return readNumber(row.average_victory_time);
        if (key === "fastest")
            return readNumber(asStatsRow(row.Fastest).length);
        if (key === "freq") return readNumber(row.frequency);
        if (key === "wins") return readNumber(row.Victory);
        if (key === "losses") return readNumber(row.Defeat);
        if (key === "win") return readNumber(row.Winrate ?? row.winrate);
        if (key === "bonus") return readNumber(row.bonus);
        return "";
    });
    const selectedMap = statsState.selectedMap
        ? (mapEntries.find(([name]) => name === statsState.selectedMap) || [
              null,
          ])[0]
        : (mapEntries[0] || [null])[0];
    const selectedMapData = selectedMap
        ? asStatsRow(mapData[selectedMap])
        : null;
    const selectedMapPreview = previewManager.map(selectedMap);
    const fastest = readFastestMapDetails(selectedMapData?.Fastest);
    const players = fastest.players;
    const commanderMastery = readCommanderMasteryLookup(
        statsPayload?.commander_mastery,
    );
    const prestigeNames = readPrestigeNameLookup(statsPayload?.prestige_names);
    const mainHandles = new Set(
        readStringArray(statsPayload?.main_handles)
            .map((handle) => normalizeHandleKey(handle))
            .filter((handle) => handle.length > 0),
    );
    let p1: FastestMapPlayer | null = players[0] || null;
    let p2: FastestMapPlayer | null = players[1] || null;
    if (
        p1 &&
        p2 &&
        mainHandles.has(normalizeHandleKey(p2.handle)) &&
        !mainHandles.has(normalizeHandleKey(p1.handle))
    ) {
        p1 = players[1];
        p2 = players[0];
    }

    return (
        <div className="stats-sub-content stats-split">
            <div className="stats-pane stats-pane-left">
                <div className="table-wrap">
                    <table className="data-table stats-dense">
                        {tableHeader(
                            [
                                {
                                    key: "name",
                                    label: languageManager.translate(
                                        "ui_stats_map_name",
                                    ),
                                },
                                {
                                    key: "avg",
                                    label: languageManager.translate(
                                        "ui_stats_avg",
                                    ),
                                },
                                {
                                    key: "fastest",
                                    label: languageManager.translate(
                                        "ui_stats_fastest",
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
                                    key: "bonus",
                                    label: languageManager.translate(
                                        "ui_stats_bonus",
                                    ),
                                },
                            ],
                            mapSort,
                            onMapSort,
                        )}
                        <tbody>
                            {mapEntries.length === 0 ? (
                                <tr>
                                    <td colSpan={8} className="empty-cell">
                                        {languageManager.translate(
                                            "ui_stats_no_map_data",
                                        )}
                                    </td>
                                </tr>
                            ) : (
                                mapEntries.map(([name, row]) => (
                                    <tr
                                        key={`map-${name}`}
                                        className={
                                            name === selectedMap
                                                ? "selected-row"
                                                : ""
                                        }
                                        onClick={() =>
                                            actions.setStatsState(
                                                (current) => ({
                                                    ...current,
                                                    selectedMap: name,
                                                }),
                                            )
                                        }
                                    >
                                        <td>
                                            {languageManager.localize(name)}
                                        </td>
                                        <td>
                                            {formatDurationSeconds(
                                                row.average_victory_time,
                                            )}
                                        </td>
                                        <td>
                                            {formatDurationSeconds(
                                                asStatsRow(row.Fastest).length,
                                            )}
                                        </td>
                                        <td>{formatPercent1(row.frequency)}</td>
                                        <td>
                                            {formatNumber(row.Victory || 0)}
                                        </td>
                                        <td>{formatNumber(row.Defeat || 0)}</td>
                                        <td>
                                            {formatPercent0(
                                                row.Winrate ?? row.winrate ?? 0,
                                            )}
                                        </td>
                                        <td>
                                            {row.detailedCount == 0
                                                ? "-"
                                                : formatPercent0(row.bonus)}
                                        </td>
                                    </tr>
                                ))
                            )}
                        </tbody>
                    </table>
                </div>
            </div>
            <div className="stats-pane stats-pane-right">
                {!selectedMapData ? (
                    <div className="stats-detail-empty">
                        {languageManager.translate("ui_stats_select_map")}
                    </div>
                ) : (
                    <div className="stats-map-detail">
                        <SelectionPreview
                            assetUrl={selectedMapPreview.url}
                            title={languageManager.localize(selectedMap)}
                            subtitle={`${formatDurationSeconds(fastest.length)} | ${languageManager.localize(fastest.enemyRace || "Unknown")}`}
                            kind="map"
                            className="stats-map-hero"
                            titleClassName="stats-map-name"
                            subtitleClassName="stats-map-sub"
                        />
                        {p1 && p2 ? (
                            <div className="stats-map-players">
                                {renderFastestMapPlayer(
                                    p1,
                                    commanderMastery,
                                    prestigeNames,
                                    "fastest-p1",
                                    languageManager,
                                )}
                                {renderFastestMapPlayer(
                                    p2,
                                    commanderMastery,
                                    prestigeNames,
                                    "fastest-p2",
                                    languageManager,
                                )}
                            </div>
                        ) : null}
                        <div className="toolbar">
                            <button
                                type="button"
                                onClick={() =>
                                    actions.revealReplay(fastest.file || "")
                                }
                                disabled={!fastest.file}
                            >
                                {languageManager.translate(
                                    "ui_stats_find_file",
                                )}
                            </button>
                            <button
                                type="button"
                                onClick={() =>
                                    actions.showReplay(fastest.file || "")
                                }
                                disabled={!fastest.file}
                            >
                                {languageManager.translate(
                                    "ui_stats_show_overlay",
                                )}
                            </button>
                        </div>
                        <p className="note stats-map-foot">{`${languageManager.localize(fastest.difficulty || "-")} | ${formatReplayTime(fastest.date)}`}</p>
                    </div>
                )}
            </div>
        </div>
    );
}

function renderStatsCommanders(
    analysis: StatisticsAnalysis,
    statsState: StatisticsState,
    actions: StatsHelpers,
    statsPayload: StatisticsPayload | null,
    allied: boolean,
    commanderSort: SortState,
    onCommanderSort: (key: string) => void,
    languageManager: LanguageManager,
    previewManager: PreviewManager,
) {
    const key = allied ? "AllyCommanderData" : "CommanderData";
    const entries = asStatsRow(analysis && analysis[key]);
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
    const selectedEntry = selectedCommander
        ? asStatsRow(entries[selectedCommander])
        : null;
    const sum = asStatsRow(entries.any);

    return (
        <div className="stats-sub-content stats-commanders-split">
            <div className="stats-pane stats-pane-left">
                <div className="table-wrap">
                    <table className="data-table stats-dense">
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
                                            ? "selected-row"
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
                            {Object.keys(sum).length > 0 ? (
                                <tr className="stats-sum-row">
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
                    <p className="note stats-right-note">
                        {languageManager.translate(
                            "ui_stats_frequency_corrected_note",
                        )}
                    </p>
                ) : null}
            </div>
            <div className="stats-pane stats-pane-right stats-commander-pane">
                {renderCommanderDetails(
                    selectedCommander,
                    selectedEntry,
                    statsPayload,
                    languageManager,
                    previewManager,
                )}
            </div>
        </div>
    );
}

function renderStatsDiffRegion(
    analysis: StatisticsAnalysis,
    regionSort: SortState,
    onRegionSort: (key: string) => void,
    difficultySort: SortState,
    onDifficultySort: (key: string) => void,
    languageManager: LanguageManager,
) {
    const regionRows = asStatsRow(analysis && analysis.RegionData);
    const regionDataBase = namedStatsRows(regionRows).sort((a, b) =>
        a[0].localeCompare(b[0]),
    );
    const regionData = sortRows(
        regionDataBase,
        regionSort,
        ([region, row], key) => {
            if (key === "region") return region;
            if (key === "frequency") return readNumber(row.frequency);
            if (key === "wins") return readNumber(row.Victory);
            if (key === "losses") return readNumber(row.Defeat);
            if (key === "winrate") return readNumber(row.winrate);
            if (key === "asc") return readNumber(row.max_asc);
            if (key === "prestiges") {
                const prestigeMap = asStatsRow(row.prestiges);
                return Object.values(prestigeMap).reduce<number>(
                    (sum, value) => sum + readNumber(value),
                    0,
                );
            }
            if (key === "maxed") {
                return Array.isArray(row.max_com) ? row.max_com.length : 0;
            }
            return "";
        },
    );
    const diffEntriesBase = orderedDifficultyEntries(
        (analysis && analysis.DifficultyData) || {},
    );
    const diffEntries = sortRows(
        diffEntriesBase,
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
        <div className="stats-sub-content">
            <div className="table-wrap">
                <table className="data-table stats-dense">
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
                            const maxCom = Array.isArray(row.max_com)
                                ? row.max_com
                                : [];
                            const prestigeMap = asStatsRow(row.prestiges);
                            const prestigeCount = Object.values(
                                prestigeMap,
                            ).reduce<number>(
                                (sum, value) => sum + readNumber(value),
                                0,
                            );
                            return (
                                <tr key={`region-${region}`}>
                                    <td>{region}</td>
                                    <td>{formatPercent0(row.frequency)}</td>
                                    <td>{formatNumber(row.Victory || 0)}</td>
                                    <td>{formatNumber(row.Defeat || 0)}</td>
                                    <td>
                                        {formatPercent0(
                                            row.winrate ?? row.Winrate ?? 0,
                                        )}
                                    </td>
                                    <td>{formatNumber(row.max_asc || 0)}</td>
                                    <td>{`${prestigeCount}/54`}</td>
                                    <td>
                                        {maxCom.length >= 4
                                            ? `${maxCom.length}/18`
                                            : maxCom
                                                  .map((name: string) =>
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
            <div className="stats-diff-wrap table-wrap">
                <table className="data-table stats-dense stats-narrow">
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
                        {diffEntries.map(([name, row]: [string, StatsRow]) => (
                            <tr key={`diff-${name}`}>
                                <td>{languageManager.localize(name)}</td>
                                <td>{formatNumber(row.Victory || 0)}</td>
                                <td>{formatNumber(row.Defeat || 0)}</td>
                                <td>{formatPercent0(row.Winrate)}</td>
                            </tr>
                        ))}
                        <tr className="stats-sum-row">
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

function renderStatsUnits(
    analysis: StatisticsAnalysis,
    statsPayload: StatisticsPayload | null,
    statsState: StatisticsState,
    actions: StatsHelpers,
    languageManager: LanguageManager,
) {
    const unitData = analysis.UnitData as UnitData | undefined;
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
            <div className="stats-detail-empty">
                <p>{detailNote}</p>
                {languageManager.translate("ui_stats_units_requires_full")}
            </div>
        );
    }
    const mainCommanders = Object.keys(unitData.main || {}).sort((a, b) =>
        a.localeCompare(b),
    );
    const allyCommanders = Object.keys(unitData.ally || {}).sort((a, b) =>
        a.localeCompare(b),
    );
    const mainCommander =
        (mainCommanders.includes(statsState.selectedUnitMainCommander)
            ? statsState.selectedUnitMainCommander
            : mainCommanders[0]) || "";
    const allyCommander =
        (allyCommanders.includes(statsState.selectedUnitAllyCommander)
            ? statsState.selectedUnitAllyCommander
            : allyCommanders[0]) || "";
    const side = statsState.selectedUnitSide || "main";
    const commander = side === "main" ? mainCommander : allyCommander;
    const source = (unitData[side] || {})[commander] || {};
    const games = Number(source.count || 0);
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

    const hiddenMindControlUnits =
        commander === "Tychus" ||
        commander === "Vorazun" ||
        commander === "Zeratul" ||
        commander === "Abathur";

    const sortFieldByHeader: Record<string, string> = {
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

    const sortingf = ([, row]: [string, UnitStatRow], sortField: string) => {
        const value = row ? row[sortField] : undefined;
        if (typeof value === "number" && Number.isFinite(value)) {
            return value;
        }
        return 0;
    };

    const entries = Object.entries(source).filter(
        ([, row]) => typeof row === "object" && row !== null,
    ) as Array<[string, UnitStatRow]>;
    const orderedEntries = (() => {
        const sorted = [...entries];
        if (sortBy === defaultUnitSort) {
            sorted.sort((a, b) =>
                sortReverse
                    ? b[0].localeCompare(a[0])
                    : a[0].localeCompare(b[0]),
            );
            return sorted;
        }
        const field = sortFieldByHeader[sortBy] || "Name";
        sorted.sort((a, b) => {
            const va = sortingf(a, field);
            const vb = sortingf(b, field);
            if (va === vb) return 0;
            return sortReverse ? vb - va : va - vb;
        });
        return sorted;
    })();

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

        return Number(row.created || 0) > 0;
    });
    const sumEntry = filteredRows.find(([name]) => name === "sum");
    const unitRows = filteredRows.filter(([name]) => name !== "sum");
    const rows = sumEntry ? [...unitRows, sumEntry] : unitRows;

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
        <div className="stats-sub-content stats-units-layout">
            <div className="stats-unit-selectors stats-unit-commanders">
                <div className="stats-unit-column">
                    <h4>{languageManager.translate("ui_stats_side_main")}</h4>
                    <div className="table-wrap">
                        <table className="data-table stats-dense stats-unit-picker-table">
                            <tbody>
                                {mainCommanders.map((name) => (
                                    <tr
                                        key={`main-${name}`}
                                        className={
                                            side === "main" &&
                                            mainCommander === name
                                                ? "selected-row"
                                                : ""
                                        }
                                        onClick={() =>
                                            actions.setStatsState(
                                                (current) => ({
                                                    ...current,
                                                    selectedUnitMainCommander:
                                                        name,
                                                    selectedUnitSide: "main",
                                                }),
                                            )
                                        }
                                    >
                                        <td>
                                            {languageManager.localize(name)}
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                </div>
                <div className="stats-unit-column">
                    <h4>{languageManager.translate("ui_stats_side_ally")}</h4>
                    <div className="table-wrap">
                        <table className="data-table stats-dense stats-unit-picker-table">
                            <tbody>
                                {allyCommanders.map((name) => (
                                    <tr
                                        key={`ally-${name}`}
                                        className={
                                            side === "ally" &&
                                            allyCommander === name
                                                ? "selected-row"
                                                : ""
                                        }
                                        onClick={() =>
                                            actions.setStatsState(
                                                (current) => ({
                                                    ...current,
                                                    selectedUnitAllyCommander:
                                                        name,
                                                    selectedUnitSide: "ally",
                                                }),
                                            )
                                        }
                                    >
                                        <td>
                                            {languageManager.localize(name)}
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
            <div className="stats-unit-table">
                <h3>
                    {translate(languageManager, "ui_stats_unit_stats_title", {
                        side: languageManager.translate(
                            side === "main"
                                ? "ui_stats_side_main"
                                : "ui_stats_side_ally",
                        ),
                        commander: languageManager.localize(commander),
                    })}
                </h3>
                <p className="note">{detailNote}</p>
                <div className="table-wrap">
                    <table className="data-table stats-dense stats-unit-table-grid">
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
                                        className="stats-sort-btn"
                                        onClick={() =>
                                            applyUnitSort(defaultUnitSort)
                                        }
                                    >
                                        {sortHeaderText(defaultUnitSort)}
                                    </button>
                                </th>
                                {[
                                    languageManager.translate(
                                        "ui_stats_created",
                                    ),
                                    languageManager.translate("ui_stats_freq"),
                                    languageManager.translate("ui_stats_lost"),
                                    languageManager.translate(
                                        "ui_stats_lost_percent",
                                    ),
                                    languageManager.translate("ui_stats_kills"),
                                    "K/D",
                                    languageManager.translate(
                                        "ui_stats_kills_percent",
                                    ),
                                ].map((field) => (
                                    <th key={`unit-header-${field}`}>
                                        <button
                                            type="button"
                                            className="stats-sort-btn stats-sort-btn-right"
                                            onClick={() => applyUnitSort(field)}
                                        >
                                            {sortHeaderText(field)}
                                        </button>
                                    </th>
                                ))}
                            </tr>
                        </thead>
                        <tbody>
                            {rows.map(([name, row]) => (
                                <tr
                                    key={`unit-${side}-${commander}-${name}`}
                                    className={
                                        name === "sum" ? "stats-sum-row" : ""
                                    }
                                >
                                    <td className="stats-unit-col-name">
                                        {name === "sum"
                                            ? `Σ (${formatNumber(games)} ${languageManager.translate("ui_stats_games_suffix")})`
                                            : languageManager.localize(name)}
                                    </td>
                                    <td className="stats-unit-col-num">
                                        {formatNumber(row.created || 0)}
                                    </td>
                                    <td className="stats-unit-col-num">
                                        {formatPercent0(row.made || 0)}
                                    </td>
                                    <td className="stats-unit-col-num">
                                        {formatNumber(row.lost || 0)}
                                    </td>
                                    <td className="stats-unit-col-num">
                                        {row.lost_percent === null ||
                                        row.lost_percent === undefined
                                            ? "-"
                                            : formatPercent0(row.lost_percent)}
                                    </td>
                                    <td className="stats-unit-col-num">
                                        {formatNumber(row.kills || 0)}
                                    </td>
                                    <td className="stats-unit-col-num">
                                        {row.KD === null || row.KD === undefined
                                            ? "-"
                                            : Number(row.KD).toFixed(1)}
                                    </td>
                                    <td className="stats-unit-col-num">
                                        {formatPercent1(
                                            row.kill_percentage || 0,
                                        )}
                                    </td>
                                </tr>
                            ))}
                        </tbody>
                    </table>
                </div>
                <p className="note stats-right-note">
                    {languageManager.translate("ui_stats_mind_control_note")}
                </p>
            </div>
        </div>
    );
}

function renderStatsAmon(
    analysis: StatisticsAnalysis,
    statsPayload: StatisticsPayload | null,
    amonSort: SortState,
    onAmonSort: (key: string) => void,
    languageManager: LanguageManager,
) {
    const unitData = analysis.UnitData as UnitData | undefined;
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
            <div className="stats-detail-empty">
                <p>{detailNote}</p>
                {languageManager.translate("ui_stats_amon_requires_full")}
            </div>
        );
    }
    const rowsBase = Object.entries(unitData.amon) as Array<
        [string, UnitStatRow]
    >;
    rowsBase.sort((a, b) => {
        if (a[0] === "sum") return -1;
        if (b[0] === "sum") return 1;
        const createdDelta =
            Number(b[1].created || 0) - Number(a[1].created || 0);
        if (createdDelta !== 0) return createdDelta;
        return String(a[0]).localeCompare(String(b[0]));
    });
    const sumRow = rowsBase.find(([name]) => name === "sum") || null;
    const detailRowsBase = rowsBase.filter(([name]) => name !== "sum");
    const detailRows = sortRows(
        detailRowsBase,
        amonSort,
        ([name, row], key) => {
            if (key === "name") return name;
            if (key === "created") return Number(row.created || 0);
            if (key === "lost") return Number(row.lost || 0);
            if (key === "kills") return Number(row.kills || 0);
            if (key === "kd") {
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
            return "";
        },
    );
    const rows = sumRow ? [sumRow, ...detailRows] : detailRows;

    return (
        <div className="stats-sub-content">
            <p className="note">{detailNote}</p>
            <div className="table-wrap">
                <table className="data-table stats-dense">
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
                                    name === "sum" ? "stats-sum-row" : ""
                                }
                            >
                                <td>
                                    {name === "sum"
                                        ? languageManager.translate(
                                              "ui_common_total",
                                          )
                                        : languageManager.localize(name)}
                                </td>
                                <td>{formatNumber(row.created || 0)}</td>
                                <td>{formatNumber(row.lost || 0)}</td>
                                <td>{formatNumber(row.kills || 0)}</td>
                                <td>
                                    {typeof row.KD === "string"
                                        ? row.KD
                                        : Number(row.KD || 0).toFixed(1)}
                                </td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </div>
        </div>
    );
}

type StatsTableSortKey =
    | "maps"
    | "ally_commanders"
    | "my_commanders"
    | "regions"
    | "difficulties"
    | "amon";

type StatsTableSortState = Record<StatsTableSortKey, SortState>;

const DEFAULT_STATS_TABLE_SORTS: StatsTableSortState = {
    maps: { key: "name", direction: "asc" },
    ally_commanders: { key: "name", direction: "asc" },
    my_commanders: { key: "name", direction: "asc" },
    regions: { key: "region", direction: "asc" },
    difficulties: { key: "difficulty", direction: "asc" },
    amon: { key: "created", direction: "desc" },
};

export default function StatisticsTab({
    statsPayload,
    statsState,
    actions,
    languageManager,
}: StatisticsTabProps) {
    const t = (id: string) => languageManager.translate(id);
    const formatText = (
        id: string,
        values: Record<string, string | number> = {},
    ): string => translate(languageManager, id, values);
    const payload = statsPayload || {};
    const analysis = payload.analysis || null;
    const gamesFound = Number(payload.games || 0);
    const previewManager = React.useMemo(
        () => new PreviewManager(languageManager),
        [languageManager],
    );

    const filterCheckbox = (
        label: string,
        checked: boolean,
        onChange: () => void,
    ) => (
        <label className="stats-checkbox-line" key={label}>
            <input type="checkbox" checked={checked} onChange={onChange} />
            <span>{label}</span>
        </label>
    );

    const subtab = STATS_SUBTABS.find(
        (item) => item.id === statsState.activeSubtab,
    )
        ? statsState.activeSubtab
        : "maps";
    const [tableSortState, setTableSortState] =
        React.useState<StatsTableSortState>(DEFAULT_STATS_TABLE_SORTS);

    const toggleTableSort = React.useCallback(
        (table: StatsTableSortKey, key: string) => {
            setTableSortState((current) => ({
                ...current,
                [table]: nextSortState(current[table], key),
            }));
        },
        [],
    );

    let subtabContent = (
        <div className="stats-detail-empty">
            {payload.message || t("ui_stats_no_statistics")}
        </div>
    );

    if (!payload.ready) {
        subtabContent = (
            <div className="stats-detail-empty">
                {t("ui_stats_no_statistics")}
            </div>
        );
    } else if (analysis) {
        if (subtab === "maps")
            subtabContent = renderStatsMaps(
                analysis,
                statsState,
                actions,
                payload,
                tableSortState.maps,
                (key) => toggleTableSort("maps", key),
                languageManager,
                previewManager,
            );
        if (subtab === "ally")
            subtabContent = renderStatsCommanders(
                analysis,
                statsState,
                actions,
                payload,
                true,
                tableSortState.ally_commanders,
                (key) => toggleTableSort("ally_commanders", key),
                languageManager,
                previewManager,
            );
        if (subtab === "my")
            subtabContent = renderStatsCommanders(
                analysis,
                statsState,
                actions,
                payload,
                false,
                tableSortState.my_commanders,
                (key) => toggleTableSort("my_commanders", key),
                languageManager,
                previewManager,
            );
        if (subtab === "diffregion")
            subtabContent = renderStatsDiffRegion(
                analysis,
                tableSortState.regions,
                (key) => toggleTableSort("regions", key),
                tableSortState.difficulties,
                (key) => toggleTableSort("difficulties", key),
                languageManager,
            );
        if (subtab === "units")
            subtabContent = renderStatsUnits(
                analysis,
                payload,
                statsState,
                actions,
                languageManager,
            );
        if (subtab === "amon")
            subtabContent = renderStatsAmon(
                analysis,
                payload,
                tableSortState.amon,
                (key) => toggleTableSort("amon", key),
                languageManager,
            );
    }

    return (
        <div className="tab-content">
            <section className="card group stats-root">
                <div className="stats-top-grid">
                    <div className="stats-check-cols">
                        <div className="stats-col">
                            {filterCheckbox(
                                languageManager.localize("Casual"),
                                statsState.filters.difficulties.Casual,
                                () => actions.toggleDifficulty("Casual"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Normal"),
                                statsState.filters.difficulties.Normal,
                                () => actions.toggleDifficulty("Normal"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Hard"),
                                statsState.filters.difficulties.Hard,
                                () => actions.toggleDifficulty("Hard"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Brutal"),
                                statsState.filters.difficulties.Brutal,
                                () => actions.toggleDifficulty("Brutal"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Brutal+1"),
                                statsState.filters.difficulties.BrutalPlus1,
                                () => actions.toggleDifficulty("BrutalPlus1"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Brutal+2"),
                                statsState.filters.difficulties.BrutalPlus2,
                                () => actions.toggleDifficulty("BrutalPlus2"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Brutal+3"),
                                statsState.filters.difficulties.BrutalPlus3,
                                () => actions.toggleDifficulty("BrutalPlus3"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Brutal+4"),
                                statsState.filters.difficulties.BrutalPlus4,
                                () => actions.toggleDifficulty("BrutalPlus4"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Brutal+5"),
                                statsState.filters.difficulties.BrutalPlus5,
                                () => actions.toggleDifficulty("BrutalPlus5"),
                            )}
                            {filterCheckbox(
                                languageManager.localize("Brutal+6"),
                                statsState.filters.difficulties.BrutalPlus6,
                                () => actions.toggleDifficulty("BrutalPlus6"),
                            )}
                        </div>
                        <div className="stats-col">
                            {filterCheckbox(
                                t("ui_stats_region_americas"),
                                statsState.filters.regions.NA,
                                () => actions.toggleRegion("NA"),
                            )}
                            {filterCheckbox(
                                t("ui_stats_region_europe"),
                                statsState.filters.regions.EU,
                                () => actions.toggleRegion("EU"),
                            )}
                            {filterCheckbox(
                                t("ui_stats_region_asia"),
                                statsState.filters.regions.KR,
                                () => actions.toggleRegion("KR"),
                            )}
                            {filterCheckbox(
                                t("ui_stats_region_china"),
                                statsState.filters.regions.CN,
                                () => actions.toggleRegion("CN"),
                            )}
                        </div>
                        <div className="stats-col">
                            {filterCheckbox(
                                t("ui_stats_normal_games"),
                                statsState.filters.includeNormalGames,
                                () =>
                                    actions.setStatsBool("includeNormalGames"),
                            )}
                            {filterCheckbox(
                                t("ui_stats_mutations"),
                                statsState.filters.includeMutations,
                                () => actions.setStatsBool("includeMutations"),
                            )}
                            {filterCheckbox(
                                t("ui_stats_wins_only"),
                                statsState.filters.winsOnly,
                                () => actions.setStatsBool("winsOnly"),
                            )}
                        </div>
                        <div className="stats-col">
                            {filterCheckbox(
                                t("ui_stats_override_folder"),
                                statsState.filters.overrideFolderSelection,
                                () =>
                                    actions.setStatsBool(
                                        "overrideFolderSelection",
                                    ),
                            )}
                            {filterCheckbox(
                                t("ui_stats_include_multibox"),
                                statsState.filters.includeMultiBox,
                                () => actions.setStatsBool("includeMultiBox"),
                            )}
                            {filterCheckbox(
                                t("ui_stats_include_levels_1_14"),
                                statsState.filters.includeSub15,
                                () => actions.setStatsBool("includeSub15"),
                            )}
                            {filterCheckbox(
                                t("ui_stats_include_levels_15_plus"),
                                statsState.filters.includeOver15,
                                () => actions.setStatsBool("includeOver15"),
                            )}
                        </div>
                    </div>
                    <div className="stats-filters-side">
                        <div className="stats-minmax">
                            <h4>{t("ui_stats_game_length_minutes")}</h4>
                            <label>
                                <input
                                    className="input"
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
                                <span>{t("ui_common_minimum")}</span>
                            </label>
                            <label>
                                <input
                                    className="input"
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
                                <span>{t("ui_common_maximum")}</span>
                            </label>
                        </div>
                        <div className="stats-dates">
                            <h4>{t("ui_stats_replay_date")}</h4>
                            <label>
                                <input
                                    className="input"
                                    type="date"
                                    value={statsState.filters.fromDate}
                                    onChange={(event) =>
                                        actions.setStatsText(
                                            "fromDate",
                                            event.target.value,
                                        )
                                    }
                                />
                                <span>{t("ui_common_from")}</span>
                            </label>
                            <label>
                                <input
                                    className="input"
                                    type="date"
                                    value={statsState.filters.toDate}
                                    onChange={(event) =>
                                        actions.setStatsText(
                                            "toDate",
                                            event.target.value,
                                        )
                                    }
                                />
                                <span>{t("ui_common_to")}</span>
                            </label>
                            <input
                                className="input"
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
                        <div className="stats-side-actions">
                            <button
                                type="button"
                                onClick={actions.startSimpleAnalysis}
                                disabled={
                                    actions.isBusy ||
                                    Boolean(payload.ready) ||
                                    Boolean(payload.simple_analysis_running) ||
                                    Boolean(payload.detailed_analysis_running)
                                }
                            >
                                {payload.simple_analysis_running
                                    ? t("ui_stats_simple_running")
                                    : t("ui_stats_run_simple_analysis")}
                            </button>
                            <button
                                type="button"
                                onClick={actions.dumpData}
                                disabled={actions.isBusy || !payload.ready}
                            >
                                {t("ui_stats_dump_data")}
                            </button>
                            <button
                                type="button"
                                onClick={actions.refreshStats}
                                disabled={actions.isBusy}
                            >
                                {actions.isBusy
                                    ? t("ui_common_loading")
                                    : t("ui_common_refresh")}
                            </button>
                            <p>
                                {formatText("ui_stats_games_found", {
                                    value: formatNumber(gamesFound),
                                })}
                            </p>
                        </div>
                    </div>
                </div>
                <nav className="stats-subtabs">
                    {STATS_SUBTABS.map((item) => (
                        <button
                            key={item.id}
                            type="button"
                            className={`stats-subtab-btn ${item.id === subtab ? "is-active" : ""}`}
                            onClick={() =>
                                actions.setStatsState((current) => ({
                                    ...current,
                                    activeSubtab: item.id,
                                }))
                            }
                        >
                            {languageManager.translate(item.titleId)}
                        </button>
                    ))}
                </nav>
                {subtabContent}
            </section>
        </div>
    );
}
