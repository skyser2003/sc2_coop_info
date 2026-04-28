import type {
    ReplayVisualPayload,
    ReplayVisualUnit,
} from "../../../bindings/overlay";

export type ReplayVisualUnitTrackSample = {
    seconds: number;
    unit: ReplayVisualUnit;
};

export type ReplayVisualUnitTrack = {
    id: string;
    samples: ReplayVisualUnitTrackSample[];
    visibleUntilSeconds: number;
};

function canInterpolateUnitIdentity(
    current: ReplayVisualUnit,
    next: ReplayVisualUnit,
): boolean {
    return (
        current.unit_type === next.unit_type &&
        current.owner_player_id === next.owner_player_id &&
        current.owner_kind === next.owner_kind &&
        current.group === next.group
    );
}

function interpolatedUnitPosition(
    current: ReplayVisualUnit,
    next: ReplayVisualUnit,
    progress: number,
): ReplayVisualUnit {
    return {
        ...current,
        x: current.x + (next.x - current.x) * progress,
        y: current.y + (next.y - current.y) * progress,
        radius: current.radius + (next.radius - current.radius) * progress,
    };
}

export function canInterpolateUnitMovement(
    current: ReplayVisualUnit,
    next: ReplayVisualUnit,
    elapsedSeconds: number,
): boolean {
    return (
        elapsedSeconds > 0 &&
        next.interpolate_from_previous &&
        canInterpolateUnitIdentity(current, next)
    );
}

function shouldAppendTrackSample(
    current: ReplayVisualUnit,
    next: ReplayVisualUnit,
): boolean {
    return (
        current.x !== next.x ||
        current.y !== next.y ||
        current.radius !== next.radius ||
        current.unit_type !== next.unit_type ||
        current.display_name !== next.display_name ||
        current.owner_player_id !== next.owner_player_id ||
        current.owner_kind !== next.owner_kind ||
        current.group !== next.group ||
        current.interpolate_from_previous !== next.interpolate_from_previous
    );
}

export function buildUnitTracks(
    payload: ReplayVisualPayload,
): readonly ReplayVisualUnitTrack[] {
    const tracks = new Map<string, ReplayVisualUnitTrack>();

    for (const frame of payload.frames) {
        for (const unit of frame.units) {
            const existing = tracks.get(unit.id);
            if (!existing) {
                tracks.set(unit.id, {
                    id: unit.id,
                    samples: [{ seconds: frame.seconds, unit }],
                    visibleUntilSeconds: frame.seconds,
                });
                continue;
            }

            existing.visibleUntilSeconds = frame.seconds;
            const currentSample = existing.samples[existing.samples.length - 1];
            if (
                currentSample &&
                shouldAppendTrackSample(currentSample.unit, unit)
            ) {
                existing.samples.push({ seconds: frame.seconds, unit });
            }
        }
    }

    return Array.from(tracks.values());
}

export function unitAtSeconds(
    track: ReplayVisualUnitTrack,
    seconds: number,
): ReplayVisualUnit | null {
    const firstSample = track.samples[0];
    if (
        !firstSample ||
        seconds < firstSample.seconds ||
        seconds > track.visibleUntilSeconds
    ) {
        return null;
    }

    if (track.samples.length === 1) {
        return firstSample.unit;
    }

    let low = 0;
    let high = track.samples.length - 1;
    while (low <= high) {
        const middle = Math.floor((low + high) / 2);
        const sample = track.samples[middle];
        if (!sample || sample.seconds > seconds) {
            high = middle - 1;
        } else {
            low = middle + 1;
        }
    }

    const currentIndex = Math.min(track.samples.length - 1, Math.max(0, high));
    const currentSample = track.samples[currentIndex];
    const nextSample = track.samples[currentIndex + 1];
    if (
        !currentSample ||
        !nextSample ||
        nextSample.seconds <= currentSample.seconds
    ) {
        return currentSample?.unit || null;
    }

    const elapsedSeconds = nextSample.seconds - currentSample.seconds;
    if (
        !canInterpolateUnitMovement(
            currentSample.unit,
            nextSample.unit,
            elapsedSeconds,
        )
    ) {
        return currentSample.unit;
    }

    const progress =
        (seconds - currentSample.seconds) /
        (nextSample.seconds - currentSample.seconds);
    return interpolatedUnitPosition(
        currentSample.unit,
        nextSample.unit,
        progress,
    );
}

export function interpolatedUnits(
    tracks: readonly ReplayVisualUnitTrack[],
    seconds: number,
): readonly ReplayVisualUnit[] {
    const units: ReplayVisualUnit[] = [];
    for (const track of tracks) {
        const unit = unitAtSeconds(track, seconds);
        if (unit) {
            units.push(unit);
        }
    }
    return units;
}
