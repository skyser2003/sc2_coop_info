# Rifts to Korhal

## Source

- Internal mission ID: `AC_KorhalRift`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `1d8e4c49476c03c2e7b2c21296938551c483bf0e601a580aca4ba9a5f3164d78`

## Primary objectives

Ten `VoidShardAC` units, controlled by player 7 in the verified revision, are split
into four timed stages:

| Stage | Shards | Deadline / nominal activation |
| ---: | ---: | ---: |
| 1 | 1 | `08:00` |
| 2 | 2 | `12:20` |
| 3 | 3 | `19:20` |
| 4 | 4 | `24:00` |

A stage completes only when all shards assigned to it are dead. If its deadline
expires with even one assigned shard alive, the map immediately fails both primary
objectives. Killing all ten wins.

Tracker reconstruction is direct for progress: bind shard births/map-initial tags to
the four stage clusters and record each combat death. Stage completion is the last
death in its `1/2/3/4` group. A loss at a deadline with a live shard is stage
failure. Nominal time alone should not overwrite an observed lifecycle.

The four stages are the correct compact milestones. A five-marker model is not
supported by this map revision.

## Bonus objectives

Two `ACPirateCapitalShip` objectives, controlled by player 8, activate near
`11:40` and `18:50`. Each has a seven-minute (`420` second) AI-time life timer.
The timer pauses while that ship has been attacked recently.

| Outcome | Map behavior | Tracker rule |
| --- | --- | --- |
| `completed` | Players destroy the capital ship before expiry. | Combat death credited to a co-op player. |
| `failed` | Timed life expires. | The ship becomes untargetable and is moved/removed; identify its killer-less lifecycle end after the unpaused duration. |
| `unresolved` | Mission ends first. | Preserve neutral state. |

The archive contains unused legacy functions for a possible third capital ship, but
normal mission initialization activates exactly two. Do not create a third bonus
slot.

## Current analyzer gap

The analyzer detects player-attributed capital-ship deaths. It does not classify
timeout failures/unresolved ships and should replace the old five-main-marker model
with four stage groups containing ten shard deaths.
