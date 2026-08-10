# Malwarfare

## Source

- Internal mission ID: `AC_CybrosEscort`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `a2a84babaebbeb6bd2dc0fc06f05e61a04a3b8ca7e78346b58b8ea27828d16ca`

## Primary objectives

The protected escort unit is `MegalithCutter`, controlled by the mission guardian
role (player 5 in the verified revision). The mission contains four Purifier lock
downloads followed by final docking; the compact five-marker model represents
those phases, not five independent primary objectives.

For each of the four locks, the guardian travels to the lock, channels through a
nominal `165`-second holdout, and then moves on. Certain map attacks can pause the
channel timer. After lock four, reaching the final dock triggers victory.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | Four lock channels finish and the guardian completes final docking. The script explicitly completes `AC_CybrosEscort_Primary01` (activate the locks). |
| `failed` | `MegalithCutter` dies. This explicitly fails Primary 01 and starts mission defeat. |

The archive also defines `AC_CybrosEscort_Primary02` (the guardian must survive)
and create/complete/fail trigger functions for it, but no normal script path invokes
its complete or fail function. For deterministic semantic output, derive Primary 02
as completed on the final-docking victory and failed on guardian death. For any
other ending, leave it `unresolved`. If the overlay is reproducing raw map objective
state instead, expose that this objective remains active because of the script
omission.

On the common base-defeat path, this archive explicitly fails Primary 01 but does
not invoke the complete/fail function for Primary 02. Preserve that raw-state
difference if the overlay displays individual campaign objective states.

Tracker events can deterministically detect guardian birth/morph/death, but not its
region entry, private channel timer, paused state, or docking. Exact lock completion
times need game events or explicit map-state instrumentation. Do not infer a lock
completion merely because a holdout enemy wave ended.

## Bonus objectives

There are two independent backup-data downloads. Each optional data center requires
three completed downloader cycles.

Lifecycle of each center:

1. The neutral center (player 6 in this revision) activates when the guardian enters
   its activation region.
2. About six seconds later, ownership changes to the active hostile bonus role,
   player 9 or 10.
3. A `CybrosEscortDownloader` completes three download cycles for success.
4. Success returns the center to player 6.
5. A `240`-second AI-time expiry or center destruction fails the objective. Timeout
   also returns the center to player 6 and makes it invulnerable.

Combat does not pause this bonus timer. The ownership transition from player 9/10
back to player 6 is therefore ambiguous by itself: it occurs for both success and
timeout. Deterministic classification requires the activation time plus lifecycle:

- center death before completion: `failed`;
- return near activation plus roughly 246 seconds: timeout `failed`;
- earlier return after three downloader cycles: `completed`;
- mission end while active or before activation: `unresolved`.

## Current analyzer gap

The current owner-change handler records activation timestamps and suppresses a
return only when elapsed time is exactly `245.9375` seconds. This captures the
verified timeout, but exact floating-point equality is fragile across revisions or
timing variation. It also does not retain destroyed, timed-out, or unresolved
states. Prefer a typed per-center lifecycle with a tolerance and explicit outcome.
