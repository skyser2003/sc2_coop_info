# Void Thrashing

## Source

- Internal mission ID: `AC_CharThrasher`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `d755532dd0f642d65d5487b64c7e9140c4665f79cc61a0e49568989945d9c173`

## Primary objectives

The mission requires ten `VoidThrasher` kills while Hammer's fortress survives.
The Thrashers are split into four groups, not four individual targets:

| Group | Thrashers | Nominal spawn deadline |
| --- | ---: | ---: |
| A | 1 | `04:30` |
| B | 2 | `09:20` |
| C | 3 | `13:40` |
| D | 4 | `18:00` |

Each target begins as a placed rumble/rift unit. Its Thrasher can spawn before the
nominal deadline when the nearby defender group's combined vitality falls to 50%
or less. Clearing the preceding group also reveals later targets early. The script
kills the rumble unit and then creates the real `VoidThrasher`; do not count that
rumble death as objective progress.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | The tenth real Void Thrasher dies; both primary objectives complete. |
| `failed` | The fixed Hammer's fortress unit dies; both primary objectives fail. |

Fortress maximum life across the six map difficulty values is
`15000, 10000, 6000, 4000, 2500, 2500`.

For tracker reconstruction, count each real `VoidThrasher` combat death and group
its birth by the `1/2/3/4` spawn site cluster. A group marker closes on its last
Thrasher death, but aggregate victory requires all ten. A four-kill model is wrong.

## Bonus objective

The Archangel is one morphing unit controlled by player 5 in the verified revision.
Its visible types are `ArchAngelCoopAssault` and `ArchAngelCoopFighter`.

- Scheduled activation is `12:30`, but damaging the unit can activate it early.
- Its timed life is `600` seconds of AI time.
- The timer pauses while the Archangel has been attacked within the previous 15
  seconds.
- Combat death in either form completes the bonus.
- Timer expiry fails the bonus and script-removes the unit.
- Mission end before death/expiry leaves it `unresolved`.

Bind the assault/fighter morphs to one tag. A co-op-attributed death is deterministic
success; a killer-less removal after the unpaused timed life is failure.

## Current analyzer gap

The analyzer records a death of either Archangel form but does not require a co-op
killer, so the timeout removal can be misclassified as success. It does not retain
timeout/unresolved state, and the main overlay model must be upgraded from four
generic markers to four groups containing ten individual kills.
