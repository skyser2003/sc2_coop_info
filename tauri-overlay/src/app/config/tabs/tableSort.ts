type SortValue = string | number | boolean | null | undefined;

export type SortDirection = "asc" | "desc";

export type SortState = {
    key: string;
    direction: SortDirection;
} | null;

function toNumeric(value: SortValue): number | null {
    if (typeof value === "number" && Number.isFinite(value)) {
        return value;
    }
    if (typeof value === "string") {
        const trimmed = value.trim();
        if (!trimmed) {
            return null;
        }
        const parsed = Number(trimmed);
        if (Number.isFinite(parsed)) {
            return parsed;
        }
    }
    return null;
}

export function compareSortValues(left: SortValue, right: SortValue): number {
    const leftNumber = toNumeric(left);
    const rightNumber = toNumeric(right);
    if (leftNumber !== null && rightNumber !== null) {
        if (leftNumber === rightNumber) {
            return 0;
        }
        return leftNumber < rightNumber ? -1 : 1;
    }

    const leftText =
        left === null || left === undefined ? "" : String(left).toLowerCase();
    const rightText =
        right === null || right === undefined
            ? ""
            : String(right).toLowerCase();
    return leftText.localeCompare(rightText, undefined, {
        numeric: true,
        sensitivity: "base",
    });
}

export function sortRows<T>(
    rows: readonly T[],
    sortState: SortState,
    valueFor: (row: T, key: string) => SortValue,
): T[] {
    if (!sortState) {
        return [...rows];
    }
    const factor = sortState.direction === "asc" ? 1 : -1;
    return [...rows].sort((left, right) => {
        const compared = compareSortValues(
            valueFor(left, sortState.key),
            valueFor(right, sortState.key),
        );
        if (compared === 0) {
            return 0;
        }
        return compared * factor;
    });
}

export function nextSortState(current: SortState, key: string): SortState {
    if (!current || current.key !== key) {
        return { key, direction: "asc" };
    }
    if (current.direction === "asc") {
        return { key, direction: "desc" };
    }
    return null;
}

export function sortIndicator(sortState: SortState, key: string): string {
    if (!sortState || sortState.key !== key) {
        return "";
    }
    return sortState.direction === "asc" ? " ▲" : " ▼";
}
