# Scythe of Amon

## Source

- Internal mission ID: `AC_AiurSiege`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `e0e545487c1c1da6a261cfb6f009084e1f83646a437b0921717cfea904fc92b9`

## Primary objectives

Five map-initial Void Slivers form the main progression. The map starts the mission
failure timer at eight minutes. Each of the first four Sliver deaths adds five
minutes; the final death stops the deadline and wins.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | All five real Void Slivers die. |
| `failed` | The shared mission-failure timer reaches zero while a Sliver remains. |

For the overlay, use each real Sliver tag as one `1/5` progress lifecycle. A combat
death is a completed milestone and, for deaths one through four, adds `05:00` to
the reconstructed deadline. Avoid matching units spawned by mutations or effects
solely by a broad name prefix; map-initial identity/position is safer.

## Bonus objectives

Three sequential escort legs are scheduled at `07:00`, `13:00`, and `16:00`. A
later leg waits when necessary for the previous escort lifecycle to stop being
active. The escort is a player-11 `WarpPrism`; a surviving prism can be reused in
the next leg, so tag/lifecycle continuity matters.

For each leg:

1. The prism travels to the evacuation point.
2. It must survive a 30-second landing/evacuation period.
3. `WarpPrism -> WarpPrismPhasing` completes that leg.
4. Death before completion fails that leg.

| Aggregate state | Meaning |
| --- | --- |
| `completed` | All three legs completed (`3/3`). |
| `partial` | One or two legs completed, with another failed or unresolved. |
| `failed` | A specific activated leg's prism died; retain that per-leg result. |
| `unresolved` | A future leg never activated, or replay end interrupted an active leg. |

Do not label all future legs failed when the main mission ends early.

## Tracker reconstruction

- Bind `WarpPrism` and `WarpPrismPhasing` morphs by unit tag.
- The morph to phasing mode is the success signal, not a prism death or later reuse.
- A death before that leg's morph is failure.
- Associate a success/failure with the currently activated leg; the same unit can
  produce more than one successful leg over time.

## Current analyzer gap

The analyzer recognizes the success morph. It needs explicit leg activation and
unit-tag state so morphs and deaths are assigned to the correct one of three slots,
including failure and unresolved states.
