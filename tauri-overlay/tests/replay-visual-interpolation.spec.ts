import { expect, test } from "@playwright/test";
import type { ReplayVisualUnit } from "../src/bindings/overlay";
import {
    canInterpolateUnitMovement,
    unitAtSeconds,
    type ReplayVisualUnitTrack,
} from "../src/app/config/tabs/replayVisualTracks";

type ReplayUnitInput = {
    id: string;
    x: number;
    y: number;
    interpolateFromPrevious: boolean;
};

function replayUnit(input: ReplayUnitInput): ReplayVisualUnit {
    return {
        id: input.id,
        unit_type: "Marine",
        display_name: "Marine",
        owner_player_id: 1,
        owner_kind: "main",
        group: "attack_units",
        x: input.x,
        y: input.y,
        radius: 0.68,
        interpolate_from_previous: input.interpolateFromPrevious,
    };
}

function unitTrack(
    firstUnit: ReplayVisualUnit,
    secondUnit: ReplayVisualUnit,
    secondSeconds: number,
): ReplayVisualUnitTrack {
    return {
        id: firstUnit.id,
        samples: [
            { seconds: 0, unit: firstUnit },
            { seconds: secondSeconds, unit: secondUnit },
        ],
        visibleUntilSeconds: secondSeconds,
    };
}

function snapUnitTrack(
    firstUnit: ReplayVisualUnit,
    snapUnit: ReplayVisualUnit,
    stableUnit: ReplayVisualUnit,
): ReplayVisualUnitTrack {
    return {
        id: firstUnit.id,
        samples: [
            { seconds: 0, unit: firstUnit },
            { seconds: 1, unit: snapUnit },
            { seconds: 2, unit: stableUnit },
        ],
        visibleUntilSeconds: 2,
    };
}

test.describe("replay visual interpolation", () => {
    test("interpolates plausible unit movement", () => {
        const start = replayUnit({
            id: "1",
            x: 10,
            y: 10,
            interpolateFromPrevious: true,
        });
        const end = replayUnit({
            id: "1",
            x: 18,
            y: 10,
            interpolateFromPrevious: true,
        });
        const halfway = unitAtSeconds(unitTrack(start, end, 1), 0.5);

        expect(canInterpolateUnitMovement(start, end, 1)).toBe(true);
        expect(halfway?.x).toBe(14);
        expect(halfway?.y).toBe(10);
    });

    test("interpolates large movement using distance over time", () => {
        const start = replayUnit({
            id: "1",
            x: 10,
            y: 10,
            interpolateFromPrevious: true,
        });
        const end = replayUnit({
            id: "1",
            x: 60,
            y: 10,
            interpolateFromPrevious: true,
        });
        const track = unitTrack(start, end, 1);

        expect(canInterpolateUnitMovement(start, end, 1)).toBe(true);
        expect(unitAtSeconds(track, 0.5)?.x).toBe(35);
        expect(unitAtSeconds(track, 1)?.x).toBe(60);
    });

    test("renders a snap movement sample immediately when it is reached", () => {
        const start = replayUnit({
            id: "1",
            x: 10,
            y: 10,
            interpolateFromPrevious: true,
        });
        const end = replayUnit({
            id: "1",
            x: 60,
            y: 10,
            interpolateFromPrevious: false,
        });
        const stable = replayUnit({
            id: "1",
            x: 60,
            y: 10,
            interpolateFromPrevious: true,
        });
        const track = snapUnitTrack(start, end, stable);

        expect(canInterpolateUnitMovement(start, end, 1)).toBe(false);
        expect(unitAtSeconds(track, 0.5)?.x).toBe(10);
        expect(unitAtSeconds(track, 1)?.x).toBe(60);
        expect(unitAtSeconds(track, 1.5)?.x).toBe(60);
        expect(unitAtSeconds(track, 2)?.x).toBe(60);
    });
});
