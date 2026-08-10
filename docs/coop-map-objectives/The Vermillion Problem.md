# The Vermillion Problem

## Source

- Internal mission ID: `AC_VeridiaCourier`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `467f5779705313b497ca37fe8dc869f66fa4f06192df1205726a810f4430cf7c`

## Primary objectives

The map requires 20 fuel-cell deliveries. Picking up a neutral
`FuelCellPickupUnit` gives the carrying unit the `FuelCellPickupUnit` behavior.
Authoritative delivery occurs when a carrier with that behavior enters drop region
61; the script increments `gv_objectsMoved`, removes the behavior, and creates the
`FuelCellTurninLM` effect.

| Difficulty | Initial active-time budget | Time added by each of the first 19 deliveries |
| --- | ---: | ---: |
| Casual / Normal | `07:00` | `02:00` |
| Hard / Brutal | `05:00` / `04:00` | `01:30` |

The twentieth delivery immediately starts victory and therefore does not need the
normal time-add branch. The failure timer pauses while the lava is up and resumes
after the lava recedes. Lava downtime is based around `04:00` and can receive a
random `-15` to `+15` second adjustment when the script selects the next interval.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | The twentieth carrier enters the drop zone; both primary objectives complete. |
| `failed` | The active-time failure timer reaches zero; both primary objectives fail. |

Crystal births, pickups, drops, and deaths are not equivalent to delivery. Tracker
events do not expose the carrier behavior or region-entry check, so deterministic
`0/20` progress requires game-event/effect data containing `FuelCellTurninLM` or an
explicit carrier behavior source. Tracker-only code may know aggregate victory but
must not manufacture individual deposits from crystal deaths.

## Bonus objective

The Molten Salamander begins its scheduled setup at `11:00`, then waits for the
surface/lava state before the `RedstoneSalamanderBurrowed` lifecycle is created. It
morphs/alternates with `RedstoneSalamander`, tunnels among three locations, becomes
hidden/invulnerable while burrowed during unsafe phases, and resurfaces later.

| Outcome | Map behavior | Tracker rule |
| --- | --- | --- |
| `completed` | The Salamander dies. | Combat death of either form, preserving its tag across morphs. |
| `unresolved` | It remains alive at mission end or never activates. | Do not infer failure. |

The analyzed normal flow never calls the defined `ObjectiveKillSalamanderFailed`
trigger and there is no Salamander deadline. Thus alive-at-end is not a
deterministic failed bonus.

## Current analyzer gap

The analyzer recognizes player-attributed deaths of both Salamander forms. Main
deposit progress needs a non-tracker event source, and the bonus must preserve its
morph lifecycle without inventing a failure at mission end.
