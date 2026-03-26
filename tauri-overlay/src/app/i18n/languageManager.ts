import languageData from "./languageData.json";
import unitTranslationData from "../../../../s2coop-analyzer/data/unit_translation_data.json";

export type AppLanguage = "en" | "ko";
type LocalizableValue = string | number | boolean | null | undefined;

type LanguageEntry = {
    en: string;
    ko: string;
    aliases?: string[];
    asset_en?: string;
};

type LanguageData = Record<string, LanguageEntry>;
type UnitTranslationEntry = {
    en: string;
    ko: string;
};
type UnitTranslationData = Record<string, UnitTranslationEntry>;

const DEFAULT_LANGUAGE: AppLanguage = "en";
const ENGLISH_LANGUAGE: AppLanguage = "en";
const entries: LanguageData = languageData as LanguageData;
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
    private readonly unitAliasToKey: Map<string, string>;

    constructor(language: string) {
        this.language = isAppLanguage(language) ? language : DEFAULT_LANGUAGE;
        this.aliasToId = new Map<string, string>();
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
        if (!id) {
            return trimmed;
        }

        return this.translate(id);
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
        if (!id) {
            return trimmed;
        }

        const entry = entries[id];
        return entry.asset_en || entry.en;
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
}

export function createLanguageManager(
    language: string = DEFAULT_LANGUAGE,
): LanguageManager {
    return new LanguageManager(language);
}
