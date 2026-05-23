import type { JsonValue } from "./types";

export function cloneJson<T>(value: T): T {
    return JSON.parse(JSON.stringify(value)) as T;
}

export function getAtPath(
    source: object | null,
    path: Array<string>,
): JsonValue | undefined {
    return path.reduce(
        (acc: JsonValue | undefined, key) =>
            acc != null && typeof acc === "object"
                ? (acc as Record<string, JsonValue>)[key]
                : undefined,
        source as JsonValue | undefined,
    );
}

export function setAtPath<T extends object>(
    source: T,
    path: Array<string>,
    value: JsonValue,
): T {
    const clone = cloneJson(source);
    let cursor = clone as Record<string, JsonValue>;
    for (let i = 0; i < path.length - 1; i += 1) {
        const key = path[i];
        if (
            cursor[key] === undefined ||
            cursor[key] === null ||
            typeof cursor[key] !== "object"
        ) {
            cursor[key] = {};
        }
        cursor = cursor[key] as Record<string, JsonValue>;
    }
    cursor[path[path.length - 1]] = value;
    return clone;
}
