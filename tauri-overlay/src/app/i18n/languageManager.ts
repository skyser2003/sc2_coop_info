import languageData from "./language_data.json";
import commanderMasteryDataJson from "./commander_mastery.json";
import unitCompositionData from "./unit_composition.json";
import unitTranslationData from "./unit_translation_data.json";

export type AppLanguage = "en" | "ko";
type LocalizableValue = string | number | boolean | null | undefined;

type LanguageEntry = {
    en: string;
    ko: string;
    aliases?: string[];
    asset_en?: string;
};

type LanguageData = Record<string, LanguageEntry>;
type UnitCompositionData = Record<string, LanguageEntry>;
type UnitTranslationEntry = {
    en: string;
    ko: string;
};
type UnitTranslationData = Record<string, UnitTranslationEntry>;
export type LocalizedCommanderMasteryLabels = {
    en: string[];
    ko: string[];
};
export type CommanderMasteryData = Record<
    string,
    LocalizedCommanderMasteryLabels
>;

const DEFAULT_LANGUAGE: AppLanguage = "en";
const ENGLISH_LANGUAGE: AppLanguage = "en";
const entries: LanguageData = languageData as LanguageData;
const commanderMasteryEntries: CommanderMasteryData =
    commanderMasteryDataJson as CommanderMasteryData;
const unitCompositionEntries: UnitCompositionData =
    unitCompositionData as UnitCompositionData;
const unitEntries: UnitTranslationData =
    unitTranslationData as UnitTranslationData;

function normalizeAliasKey(value: string): string {
    return value
        .normalize("NFKC")
        .trim()
        .replace(/\s+/g, " ")
        .replace(/[’`]/g, "'")
        .toLocaleLowerCase("en-US");
}

function isAppLanguage(value: string): value is AppLanguage {
    return value === "en" || value === "ko";
}

export class LanguageManager {
    private language: AppLanguage;
    private readonly aliasToId: Map<string, string>;
    private readonly unitCompositionAliasToId: Map<string, string>;
    private readonly unitAliasToKey: Map<string, string>;

    constructor(language: string) {
        this.language = isAppLanguage(language) ? language : DEFAULT_LANGUAGE;
        this.aliasToId = new Map<string, string>();
        this.unitCompositionAliasToId = new Map<string, string>();
        this.unitAliasToKey = new Map<string, string>();

        for (const [id, entry] of Object.entries(entries)) {
            this.aliasToId.set(normalizeAliasKey(id), id);
            this.aliasToId.set(normalizeAliasKey(entry.en), id);
            this.aliasToId.set(normalizeAliasKey(entry.ko), id);

            if (Array.isArray(entry.aliases)) {
                for (const alias of entry.aliases) {
                    this.aliasToId.set(normalizeAliasKey(alias), id);
                }
            }
        }

        for (const [key, entry] of Object.entries(unitEntries)) {
            this.unitAliasToKey.set(normalizeAliasKey(key), key);
            this.unitAliasToKey.set(normalizeAliasKey(entry.en), key);
            this.unitAliasToKey.set(normalizeAliasKey(entry.ko), key);
        }

        for (const [id, entry] of Object.entries(unitCompositionEntries)) {
            this.unitCompositionAliasToId.set(normalizeAliasKey(id), id);
            this.unitCompositionAliasToId.set(normalizeAliasKey(entry.en), id);
            this.unitCompositionAliasToId.set(normalizeAliasKey(entry.ko), id);

            if (Array.isArray(entry.aliases)) {
                for (const alias of entry.aliases) {
                    this.unitCompositionAliasToId.set(
                        normalizeAliasKey(alias),
                        id,
                    );
                }
            }
        }
    }

    currentLanguage(): AppLanguage {
        return this.language;
    }

    setLanguage(language: string): void {
        if (isAppLanguage(language)) {
            this.language = language;
        }
    }

    translate(id: string): string {
        const entry = entries[id];
        if (!entry) {
            return id;
        }
        return entry[this.language] || entry[ENGLISH_LANGUAGE] || id;
    }

    idFromValue(value: LocalizableValue): string | null {
        if (typeof value !== "string") {
            return null;
        }

        const trimmed = value.trim();
        if (trimmed === "") {
            return null;
        }

        return this.aliasToId.get(normalizeAliasKey(trimmed)) || null;
    }

    private unitCompositionIdFromValue(value: LocalizableValue): string | null {
        if (typeof value !== "string") {
            return null;
        }

        const trimmed = value.trim();
        if (trimmed === "") {
            return null;
        }

        return (
            this.unitCompositionAliasToId.get(normalizeAliasKey(trimmed)) ||
            null
        );
    }

    localize(value: LocalizableValue): string {
        if (value === null || value === undefined) {
            return "";
        }

        if (typeof value !== "string") {
            return String(value);
        }

        const trimmed = value.trim();
        if (trimmed === "") {
            return "";
        }

        const id = this.idFromValue(trimmed);
        if (id) {
            return this.translate(id);
        }

        const unitCompositionId = this.unitCompositionIdFromValue(trimmed);
        if (!unitCompositionId) {
            return trimmed;
        }

        const entry = unitCompositionEntries[unitCompositionId];
        return entry?.[this.language] || entry?.[ENGLISH_LANGUAGE] || trimmed;
    }

    localizeUnitName(value: LocalizableValue): string {
        if (value === null || value === undefined) {
            return "";
        }

        if (typeof value !== "string") {
            return String(value);
        }

        const trimmed = value.trim();
        if (trimmed === "") {
            return "";
        }

        const key = this.unitAliasToKey.get(normalizeAliasKey(trimmed));
        if (!key) {
            return trimmed;
        }

        const entry = unitEntries[key];
        if (!entry) {
            return trimmed;
        }

        const localizedName = entry[this.language];

        if (localizedName && localizedName.length !== 0) {
            return localizedName;
        } else {
            return entry[ENGLISH_LANGUAGE] || trimmed;
        }
    }

    englishLabel(value: LocalizableValue): string {
        if (value === null || value === undefined) {
            return "";
        }

        if (typeof value !== "string") {
            return String(value);
        }

        const trimmed = value.trim();
        if (trimmed === "") {
            return "";
        }

        const id = this.idFromValue(trimmed);
        if (id) {
            const entry = entries[id];
            return entry.asset_en || entry.en;
        }

        const unitCompositionId = this.unitCompositionIdFromValue(trimmed);
        if (!unitCompositionId) {
            return trimmed;
        }

        const entry = unitCompositionEntries[unitCompositionId];
        return entry?.asset_en || entry?.en || trimmed;
    }

    localizeMapRacePair(value: LocalizableValue): string {
        if (typeof value !== "string") {
            return this.localize(value);
        }

        const parts = value
            .split("|")
            .map((part) => this.localize(part))
            .filter((part) => part !== "");
        if (parts.length === 0) {
            return "";
        }
        return parts.join(" | ");
    }

    commanderMasteryLabels(commander: string): string[] {
        const labels = commanderMasteryEntries[commander];
        if (labels === undefined) {
            return [];
        }

        return labels[this.language].length > 0
            ? labels[this.language]
            : labels.en;
    }

    commanderMasteryData(): CommanderMasteryData {
        return commanderMasteryEntries;
    }
}

export function createLanguageManager(
    language: string = DEFAULT_LANGUAGE,
): LanguageManager {
    return new LanguageManager(language);
}
