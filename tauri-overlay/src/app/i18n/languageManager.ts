import languageData from "./languageData.json";

export type AppLanguage = "en" | "ko";

type LanguageEntry = {
    en: string;
    ko: string;
    aliases?: string[];
    asset_en?: string;
};

type LanguageData = Record<string, LanguageEntry>;

const DEFAULT_LANGUAGE: AppLanguage = "en";
const ENGLISH_LANGUAGE: AppLanguage = "en";
const entries: LanguageData = languageData as LanguageData;

function normalizeAliasKey(value: string): string {
    return value
        .normalize("NFKC")
        .trim()
        .replace(/\s+/g, " ")
        .replace(/[’`]/g, "'")
        .toLocaleLowerCase("en-US");
}

function isAppLanguage(value: unknown): value is AppLanguage {
    return value === "en" || value === "ko";
}

export class LanguageManager {
    private language: AppLanguage;
    private readonly aliasToId: Map<string, string>;

    constructor(language: string) {
        this.language = isAppLanguage(language) ? language : DEFAULT_LANGUAGE;
        this.aliasToId = new Map<string, string>();

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
    }

    currentLanguage(): AppLanguage {
        return this.language;
    }

    setLanguage(language: unknown): void {
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

    idFromValue(value: unknown): string | null {
        if (typeof value !== "string") {
            return null;
        }

        const trimmed = value.trim();
        if (trimmed === "") {
            return null;
        }

        return this.aliasToId.get(normalizeAliasKey(trimmed)) || null;
    }

    localize(value: unknown): string {
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

    englishLabel(value: unknown): string {
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

    localizeMapRacePair(value: unknown): string {
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
