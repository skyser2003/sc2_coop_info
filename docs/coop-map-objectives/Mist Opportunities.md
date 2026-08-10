# Mist Opportunities

## Source

- Internal mission ID: `AC_BelshirEscort`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `9486c9499dc71914fbfd9dea497d0f51d6e4015e527cf6cfec3b17fb326459eb`

## Primary objectives

Five harvesting waves contain eleven `TerrazineHarvester` bots controlled by the
mission role (player 6 in this revision).

| Wave | Nominal activation | Bots / nodes |
| ---: | ---: | ---: |
| 1 | `03:30` | 1 |
| 2 | `06:45` | 2 |
| 3 | `11:00` | 2 |
| 4 | `17:00` | 3 |
| 5 | `23:39` | 3 |

Each bot travels to its node, harvests for `60` seconds, and returns. A successful
bot is script-removed after its return. A wave advances only after every bot thread
has resolved, whether its bots returned or died.

| Wave state | Definition |
| --- | --- |
| `completed` | Every bot in the wave returned successfully. |
| `mixed` | One or more bots died, but the global loss limit was not reached and the remaining wave resolved. |
| `failed-terminal` | A death brought the global bot-loss count to its defeat threshold. |
| `unresolved` | Replay end left at least one bot lifecycle active. |

The global defeat threshold is three lost bots on Casual/Normal/Hard and two on
Brutal. Finishing wave five wins. On victory, both displayed primary objectives
(`Primary01`, finish five waves; `Primary02`, keep losses below the limit) are set
complete even if allowed bot losses occurred. Reaching the loss threshold causes
defeat and explicitly fails Primary 01; the threshold itself deterministically
implies Primary 02 failure, although this archive does not explicitly set its
campaign state to failed.

On the common base-defeat path, the archive likewise explicitly fails Primary 01
but does not write a terminal campaign state for Primary 02.

Tracker reconstruction must distinguish combat deaths from the killer-less scripted
removal after a successful return. Birth clusters define wave membership more
reliably than exact nominal timestamps. Travel/harvest/return phase is private map
state and is not exactly visible in tracker events.

## Bonus objectives

Two `COOPTerrazineTank` objectives (the terrazine creatures) activate nominally at
about `11:18` and `23:09`. Each receives a `240`-second timer after a short setup
delay. The timer pauses while its target has been attacked recently.

| Outcome | Map behavior | Tracker rule |
| --- | --- | --- |
| `completed` | Players kill the target before timeout. | Combat death credited to a co-op player. |
| `failed` | Its timer expires. | Script applies the timeout-death effect; classify using killer and active unpaused duration. |
| `unresolved` | Mission ends first. | No state should be inferred. |

Both paths can produce a death event, so unit type alone is insufficient.

## Current analyzer gap

The analyzer recognizes co-op-attributed bonus deaths but does not expose timeout
failures. Main bot deaths must never be labeled as successful enemy-objective kills;
they are losses within a wave.
