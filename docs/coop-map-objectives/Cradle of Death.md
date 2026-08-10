# Cradle of Death

## Source

- Internal mission ID: `AC_CradleOfDeath`
- Verified replay build: `97563`
- Cached map archive SHA-256:
  `a1008367f74b6b270bbd35815d29fb371e97d8c420bd8f2b8871c60c9cf2997e`

## Primary objectives

Five facility sites exist, but the selected route requires four. Every selected
facility needs both players' `CODFlatbedTruck` payloads delivered before it is
destroyed.

| Site | Target unit | Approximate target position |
| ---: | --- | --- |
| 1 | `HybridHoldingCellSmallUnit` | `(132, 194)` |
| 2 | `PlatformPowerCore` | `(42, 157)` |
| 3 | `JoriumStockpile` | `(227, 125)` |
| 4 | `TerrazineTank` | `(65, 66)` |
| 5 | `COOPTerrazineTank` | `(211, 67)` |

The branch/order logic chooses four of the five sites. At a selected site:

1. The first payload arrival is progress `1/2`, not completion.
2. The second payload arrival completes that facility.
3. A fuse runs, the target is destroyed by the script, and time is added.
4. Completing the fourth selected facility wins the mission.

The countdown is assembled by adding a contribution for each human player's
difficulty. Per player, Brutal contributes `03:45` initial and `03:45` per
facility; Hard contributes `04:15` initial and `04:45` per facility; lower tiers
contribute `05:15` initial and `05:15` per facility. Mixed-difficulty games sum
the two appropriate contributions. The post-delivery explosion fuse similarly
adds `12`, `14`, or `15` seconds per player by those tiers.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | Four selected facilities receive both payloads. |
| `failed` | The global countdown expires before completion. |

Tracker events do not expose a unit entering a region. The target's scripted
explosion/death is strong post-completion evidence, but it must not be described
as an enemy construct killed by the players. Route selection and first-payload
progress require game events or another source beyond tracker events.

## Bonus objectives

Two independently timed bonus deliveries target `LogisticsHeadquarters` sites.
Each has a guardian Xel'Naga Construct and a seven-minute AI-time timer.

- Destroying the guardian only opens the drop-off beacon; it is not bonus success.
- The timer pauses while the guardian is recently attacked or disabled, with a
  short pause grace period.
- A truck reaching the opened beacon completes the objective. The headquarters
  then runs its completion animation/explosion; its tracker death appears about
  eight seconds after the logical completion.
- Timer expiry fails that bonus. Mission end before either condition leaves it
  `unresolved`.

For tracker-only reconstruction, use each `LogisticsHeadquarters` scripted death as
a success marker and shift its displayed logical time roughly eight seconds
earlier. A guardian death alone must never produce success.

## Current analyzer gap

The analyzer already derives a headquarters completion time from its death. It
does not expose route choice, per-facility `0/2` to `2/2` progress, delivery-timeout
failures, or unresolved bonus states.
