import type { LanguageManager } from "../i18n/languageManager";

export type PreviewKind = "map" | "commander";

export type PreviewEntry = {
    id: string;
    rawValue: string;
    assetName: string;
    url: string;
};

export type MapRacePair = {
    map: string;
    race: string;
};

function readText(value: unknown): string {
    if (value === null || value === undefined) {
        return "";
    }
    if (typeof value !== "string") {
        return String(value);
    }
    return value.trim();
}

export class PreviewManager {
    private readonly languageManager: LanguageManager;

    constructor(languageManager: LanguageManager) {
        this.languageManager = languageManager;
    }

    commander(value: unknown): PreviewEntry {
        return this.buildPreviewEntry(value, "commander");
    }

    map(value: unknown): PreviewEntry {
        return this.buildPreviewEntry(value, "map");
    }

    splitMapRacePair(value: unknown): MapRacePair {
        if (typeof value !== "string") {
            return { map: "", race: "" };
        }

        const parts = value
            .split("|")
            .map((part) => part.trim())
            .filter((part) => part !== "");
        const result: MapRacePair = { map: "", race: "" };

        for (const part of parts) {
            const id = this.languageManager.idFromValue(part);
            if (id?.startsWith("map_")) {
                if (result.map === "") {
                    result.map = part;
                }
                continue;
            }
            if (id?.startsWith("race_")) {
                if (result.race === "") {
                    result.race = part;
                }
                continue;
            }
            if (result.map === "") {
                result.map = part;
                continue;
            }
            if (result.race === "") {
                result.race = part;
            }
        }

        return result;
    }

    private buildPreviewEntry(value: unknown, kind: PreviewKind): PreviewEntry {
        const rawValue = readText(value);
        const assetName = this.languageManager.englishLabel(rawValue);
        const id = this.languageManager.idFromValue(rawValue) || rawValue;
        const extension = kind === "commander" ? ".png" : ".jpg";
        const folder = kind === "commander" ? "Commanders" : "Maps";

        return {
            id,
            rawValue,
            assetName,
            url:
                assetName === ""
                    ? ""
                    : `/overlay/${folder}/${encodeURIComponent(assetName)}${extension}`,
        };
    }
}
