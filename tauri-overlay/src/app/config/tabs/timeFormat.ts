import type { DisplayValue } from "../types";

type ReplayTimestampFormatOptions = {
    includeSeconds: boolean;
};

function paddedDatePart(value: number): string {
    return String(value).padStart(2, "0");
}

export function formatReplayTimestampLocal(
    value: DisplayValue,
    options: ReplayTimestampFormatOptions,
): string {
    const seconds = Number(value);
    if (!Number.isFinite(seconds) || seconds <= 0) {
        return "-";
    }

    const date = new Date(seconds * 1000);
    if (Number.isNaN(date.getTime())) {
        return "-";
    }

    const dateText = [
        date.getFullYear(),
        paddedDatePart(date.getMonth() + 1),
        paddedDatePart(date.getDate()),
    ].join("-");
    const timeParts = [
        paddedDatePart(date.getHours()),
        paddedDatePart(date.getMinutes()),
    ];
    if (options.includeSeconds) {
        timeParts.push(paddedDatePart(date.getSeconds()));
    }
    return `${dateText} ${timeParts.join(":")}`;
}
