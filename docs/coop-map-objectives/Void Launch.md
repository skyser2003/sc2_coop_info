# Void Launch

## Source

- Internal mission ID: `AC_KaldirShuttle`
- Verified replay builds: `96883` and `97563`
- Cached map archive SHA-256:
  `e0f8031e8400c83ab6bf7d06e32b17348507fde93ebfc177ee5395e3b92736e9`

## Primary objectives

Seven waves contain 35 `ProtossFrigate` shuttles.

| Wave | Nominal launch | Shuttle count |
| ---: | ---: | ---: |
| 1 | `06:15` | 2 |
| 2 | `09:00` | 3 |
| 3 | `12:30` | 3 |
| 4 | `15:30` | 6 |
| 5 | `18:00` | 6 |
| 6 | `20:30` | 5 |
| 7 | `23:00` | 10 |

A wave resolves when none of its shuttles remain alive. Both destroyed and escaped
shuttles remove a unit from that wave, so resolution alone is not success.

| Wave state | Definition |
| --- | --- |
| `completed` | Every shuttle in the wave was destroyed. |
| `mixed` | At least one shuttle escaped and the remaining shuttles were destroyed/escaped, below the terminal escape limit. |
| `unresolved` | Replay/mission end leaves one or more launched shuttles alive. |

A shuttle escapes when it completes `ProtossWarpAway`. Escape thresholds across the
six map difficulty values are `10, 8, 6, 5, 3, 2`; reaching the value causes defeat,
so the maximum tolerated counts are `9, 7, 5, 4, 2, 1`. Shuttle shields are
`100, 150, 200, 300, 400, 500`, and life is
`150, 200, 300, 400, 500, 600` across the same tiers.

| Aggregate outcome | Authoritative map condition |
| --- | --- |
| `completed` | All seven waves resolve below the escape threshold. Both primary objectives complete. |
| `failed` | Escape count reaches the threshold. Both primary objectives fail. |

A winning replay can contain one or more `mixed` waves because some escapes are
allowed.

## Tracker reconstruction

- `ProtossFrigate` births grouped into the seven count clusters define wave
  membership more reliably than timestamp comparisons.
- An escape appears as a shuttle death credited to the Warp Conduit controller
  (player 6 in verified replays) at `(18, 64)`, `(72, 54)`, or `(132, 56)`.
- Other shuttle deaths are destructions. Resolve the conduit role from replay slot
  identity rather than permanently hard-coding player 6.
- Record each shuttle exactly once and close a wave only after all expected tags
  resolve.

## Bonus objectives

Research vessels activate at approximately `08:15`, `14:45`, and `19:45`. Each
must travel to its shrine, land, and survive a 60-second scan.

| Per-vessel state | Map behavior | Tracker rule |
| --- | --- | --- |
| `completed` | Scan finishes and vessel lifts off. | `ResearchVessel -> ResearchVesselLanded`, then about 60 seconds later `ResearchVesselLanded -> ResearchVessel`. Ignore the killer-less scripted removal eight seconds later. |
| `failed` | Flying vessel dies en route or landed vessel dies before scan completion. | Death before the success lift-off for that tag. |
| `unresolved` | It has not activated or remains active when the mission ends. | Preserve neutral state. |

Aggregate bonus state is `completed` only at `3/3`, explicitly `failed` only after
all three are lost (`0/3`), and `partial` at `1/3` or `2/3`.

## Current analyzer gap

The short-window landed-to-flying morph logic finds successful scans, and shuttle
kills are counted. It does not yet retain per-vessel failure/unresolved state, group
the 35 shuttles into seven waves, or classify conduit-attributed deaths as escapes.
