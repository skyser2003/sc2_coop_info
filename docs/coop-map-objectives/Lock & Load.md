# Lock & Load

## Source

- Internal mission ID: `AC_UlnarLocks`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `b3d3ab918516c9f99305ba8667e7a11db7bb5ff93f060be775327f28b17ef8c6`

## Primary objectives

There are five Celestial Locks. Each can be inactive, player-controlled, or
enemy-controlled; captured locks can regress before the mission ends. The map also
tracks a global overload counter.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | All five locks are simultaneously player-controlled. |
| `failed` | The overload counter reaches `9000`. |

Both players are required for the player capture interaction. Enemy occupation can
deactivate or recapture a lock. Once at least one lock is enemy-controlled, each
periodic update adds progress for every enemy-controlled lock; more hostile locks
therefore accelerate overload.

The tracker stream does not expose the animation/private-state changes that define
lock ownership, presence in a lock region, or partial capture progress. It is not
possible to reconstruct the five lock timeline deterministically from tracker
events alone. Use game-event data if exact lock transitions are required. A replay
loss alone does not establish that overload occurred.

## Bonus objective

The bonus `XelNagaConstruct` is controlled by the neutral/mission role (player 3 in
the verified revision). It is revealed at `08:00` or earlier by proximity.

| Outcome | Authoritative condition | Tracker rule |
| --- | --- | --- |
| `completed` | The Construct dies. | Its `SUnitDiedEvent` is the success marker. |
| `failed` | Overload or the common base-defeat condition fires while the Construct is still alive. | Infer only when the map defeat is known and no earlier Construct death occurred. |
| `unresolved` | Replay interruption occurs without a map victory/defeat or Construct death. | Keep neutral. |

## Current analyzer gap

The analyzer detects Construct death as success. It cannot currently reconstruct
lock states or overload and consequently cannot distinguish the overload failure
from the common base-defeat path without another signal.
