import * as React from "react";

export type RaceIconKind = "terran" | "zerg" | "protoss" | "random" | "unknown";

type RaceIconProps = {
    label: string | null | undefined;
    className?: string;
};

function imagePathForRace(kind: RaceIconKind): string {
    if (kind === "terran") {
        return "/race-icons/terran.svg";
    }
    if (kind === "zerg") {
        return "/race-icons/zerg.svg";
    }
    if (kind === "protoss") {
        return "/race-icons/protoss.svg";
    }
    return "/race-icons/unknown.svg";
}

function normalizeLabel(value: string): string {
    return value.trim().toLowerCase();
}

export function raceIconKindForLabel(
    label: string | null | undefined,
): RaceIconKind {
    if (label == null) {
        return "unknown";
    }

    const normalized = normalizeLabel(label);
    if (normalized === "") {
        return "unknown";
    }

    if (normalized === "terran") {
        return "terran";
    }
    if (normalized === "zerg") {
        return "zerg";
    }
    if (normalized === "protoss") {
        return "protoss";
    }
    if (normalized === "random") {
        return "random";
    }

    return "unknown";
}

export function RaceIcon({
    label,
    className,
}: RaceIconProps): React.ReactElement {
    const kind = raceIconKindForLabel(label);
    const imagePath = imagePathForRace(kind);
    const generatedId = React.useId();
    const maskId = `race-icon-mask-${generatedId.replace(/:/g, "")}`;

    return (
        <svg
            aria-hidden="true"
            className={className}
            data-race={kind}
            focusable="false"
            viewBox="0 0 256 256"
        >
            <mask
                height="256"
                id={maskId}
                maskUnits="userSpaceOnUse"
                width="256"
                x="0"
                y="0"
            >
                <image
                    height="256"
                    href={imagePath}
                    preserveAspectRatio="xMidYMid meet"
                    width="256"
                    x="0"
                    y="0"
                />
            </mask>
            <rect
                fill="currentColor"
                height="256"
                mask={`url(#${maskId})`}
                width="256"
                x="0"
                y="0"
            />
        </svg>
    );
}
