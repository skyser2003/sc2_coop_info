import * as React from "react";
import type {
    GamesRowPayload,
    LocalizedText,
    UiMutatorRow,
} from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import type { DisplayValue, MutatorData } from "../types";
import styles from "../configStyles";

type GamesActionIconName = "overlay" | "visualizer" | "chatting" | "file";

type GamesActionIconProps = {
    name: GamesActionIconName;
};

type GamesActionButtonProps = {
    label: string;
    iconName: GamesActionIconName;
    disabled: boolean;
    onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
};

export function asTableValueCompat(value: DisplayValue): string {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

export function formatDurationSecondsCompat(value: DisplayValue): string {
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

export function mutatorIconPath(iconName: string): string {
    return `/overlay/Mutator Icons/${encodeURIComponent(iconName)}.png`;
}

function GamesActionIcon({ name }: GamesActionIconProps) {
    const commonProps: React.SVGProps<SVGSVGElement> = {
        "aria-hidden": true,
        focusable: "false",
        viewBox: "0 0 24 24",
        fill: "none",
        stroke: "currentColor",
        strokeWidth: 2,
        strokeLinecap: "round",
        strokeLinejoin: "round",
    };

    if (name === "overlay") {
        return (
            <svg {...commonProps}>
                <path d="M4 18V8" />
                <path d="M8 18V5" />
                <path d="M12 18v-8" />
                <path d="M16 18V7" />
                <path d="M20 18v-5" />
                <path d="M3 18h18" />
            </svg>
        );
    }

    if (name === "visualizer") {
        return (
            <svg {...commonProps}>
                <path d="M8 5v14l11-7Z" />
            </svg>
        );
    }

    if (name === "chatting") {
        return (
            <svg {...commonProps}>
                <path d="M21 12a8 8 0 0 1-8 8H7l-4 3v-7a8 8 0 1 1 18-4Z" />
                <path d="M8 11h8" />
                <path d="M8 15h5" />
            </svg>
        );
    }

    return (
        <svg {...commonProps}>
            <path d="M3 7a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v1H3Z" />
            <path d="M3 10h18l-2 9H5Z" />
        </svg>
    );
}

export function GamesActionButton({
    label,
    iconName,
    disabled,
    onClick,
}: GamesActionButtonProps) {
    return (
        <button
            type="button"
            className={[
                styles.gamesRowBtn,
                styles.gamesActionIconBtn,
                styles.buttonNormal,
            ]
                .filter(Boolean)
                .join(" ")}
            disabled={disabled}
            onClick={onClick}
            aria-label={label}
            title={label}
        >
            <GamesActionIcon name={iconName} />
        </button>
    );
}

export function readMutators(
    value: readonly UiMutatorRow[] | null | undefined,
): readonly UiMutatorRow[] {
    if (!Array.isArray(value)) {
        return [];
    }
    return value;
}

export function localizedMutatorName(
    mutator: MutatorData,
    languageManager: LanguageManager,
    asTableValue: (value: DisplayValue) => string,
): string {
    return asTableValue(
        languageManager.localizedValue(
            mutator.name as LocalizedText | null | undefined,
        ),
    );
}

export function localizedMutatorDescription(
    mutator: MutatorData,
    languageManager: LanguageManager,
    asTableValue: (value: DisplayValue) => string,
): string {
    return asTableValue(
        languageManager.localizedValue(
            mutator.description as LocalizedText | null | undefined,
        ),
    );
}

export function difficultyDisplayLabel(
    row: GamesRowPayload,
    languageManager: LanguageManager,
): string {
    const brutalPlus = Number(row.brutal_plus ?? 0);
    const suffixes: string[] = [];
    if (row.weekly === true) {
        suffixes.push(languageManager.translate("ui_overlay_weekly"));
    } else if (row.extension === true) {
        suffixes.push(languageManager.translate("ui_overlay_custom"));
    }
    const modeSuffix = suffixes.length > 0 ? ` (${suffixes.join(", ")})` : "";
    if (Number.isFinite(brutalPlus) && brutalPlus > 0) {
        return `${languageManager.localize(`B+${Math.min(6, brutalPlus)}`)}${modeSuffix}`;
    }
    return `${languageManager.localizeDifficulty(row.difficulty)}${modeSuffix}`;
}
