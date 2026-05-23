import type {
    StatsDifficultyDataRow,
    StatsRegionDataRow,
} from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import type { DisplayValue, StatisticsAnalysis } from "../types";
import { formatReplayTimestampLocal } from "./timeFormat";

type TranslationValues = {
    readonly [key: string]: string | number;
};

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

export function readNumber(value: DisplayValue, fallback: number = 0): number {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : fallback;
}

export function formatNumber(value: DisplayValue): string {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return asTableValue(value);
    }
    return num.toLocaleString("en-US");
}

export function formatPercent0(value: DisplayValue): string {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "-";
    }
    return `${(num * 100).toFixed(0)}%`;
}

export function formatPercent1(value: DisplayValue): string {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return "-";
    }
    return `${(num * 100).toFixed(1)}%`;
}

export function formatDurationSeconds(value: DisplayValue): string {
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

export function formatReplayTime(value: DisplayValue): string {
    return formatReplayTimestampLocal(value, { includeSeconds: true });
}

export function finiteNumberOrNull(value: DisplayValue): number | null {
    const num = Number(value);
    return Number.isFinite(num) ? num : null;
}

export function masteryLabelsForLanguage(
    languageManager: LanguageManager,
    commander: string,
): string[] {
    return languageManager.commanderMasteryLabels(commander);
}

export function asTableValue(value: DisplayValue): string {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

export function translate(
    languageManager: LanguageManager,
    id: string,
    values: TranslationValues = {},
): string {
    return Object.entries(values).reduce(
        (text, [key, value]) => text.split(`{{${key}}}`).join(String(value)),
        languageManager.translate(id),
    );
}

export type RegionStatsRow = readonly [string, StatsRegionDataRow];
export type DifficultyStatsRow = readonly [string, StatsDifficultyDataRow];

export function regionStatsRows(
    analysis: StatisticsAnalysis,
): RegionStatsRow[] {
    return Object.entries(analysis.RegionData).sort((a, b) =>
        a[0].localeCompare(b[0]),
    );
}

export function orderedDifficultyRows(
    analysis: StatisticsAnalysis,
): DifficultyStatsRow[] {
    const rows: DifficultyStatsRow[] = [];
    const difficultyRows = analysis.DifficultyData;
    const existing = Object.keys(difficultyRows);
    const seen = new Set<string>();

    for (const name of DIFFICULTY_ORDER) {
        const row = difficultyRows[name];
        if (row) {
            seen.add(name);
            rows.push([name, row]);
        }
    }

    for (const name of existing) {
        if (seen.has(name)) {
            continue;
        }

        if (
            name === "B+" ||
            name.toLowerCase().startsWith("brutal+") ||
            /^B\+\d+$/.test(name)
        ) {
            rows.push([name, difficultyRows[name]]);
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

export function difficultySortRank(
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
