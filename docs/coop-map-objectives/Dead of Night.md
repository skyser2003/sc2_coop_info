# Dead of Night

## Source

- Internal mission ID: `AC_MeinhoffDayNight`
- Verified replay build: `97563`
- Cached map archive SHA-256:
  `d2dc0fa6e0bc390050e511a2f11f184e6c8c936871b0579231660283f30ee9e1`

## Primary objective

The single progression objective is to destroy every infestation structure. At
initialization the script builds the count from living player-5 units of these
types:

- `InfestableBiodome`
- `InfestableHut`
- `InfestedMercHaven`
- `NovaInfestableColonistHut`
- `JarbanInfestibleColonistHut`

The total is map-revision dependent. Many replays contain 151, but the overlay
must calculate the population from unit lifecycles rather than hard-code 151.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | No living unit remains in the infestation group. |

An infestation death is one progress event. Initial births/map-initial units form
the denominator; later morphs or replacement events must retain unit identity so
they are not double-counted.

## Day/night timeline

Day/night transitions are not separate primary objectives, but are often more
useful overlay markers than 100+ structure deaths.

- Day 1 lasts `03:30`.
- Every night lasts `04:00`.
- Subsequent days last `04:00` on the lower two difficulty tiers and `03:30` on
  Hard/Brutal.
- Warning/UI transition logic begins about 40 seconds before the state changes;
  the authoritative day/night state flips only when the timer expires.

Tracker events do not directly carry the private day/night timer. Reconstruct the
sequence from the mission clock and verified difficulty/build, and label it
inferred. Do not use the warning offset as the transition time.

## Bonus objective

The Virophage activates 15 seconds into Night 3. It is `ACVirophage` while active
and can morph to `ACVirophageBurrowed` during daytime/end-of-night hiding. It can
resurface on Night 4 and later without becoming a new objective lifecycle.

| Outcome | Map behavior | Tracker rule |
| --- | --- | --- |
| `completed` | The Virophage dies. | Match a combat death of either lifecycle form, preserving its tag across morphs. |
| `unresolved` | The mission ends while it is alive, or before Night 3. | Do not turn this into failure. |

The analyzed script defines a bonus failure trigger but does not call it from the
normal Virophage lifecycle. There is no independent timeout failure. Therefore an
unkilled Virophage is incomplete/unresolved rather than deterministically failed.

## Current analyzer gap

The analyzer recognizes a co-op kill of `ACVirophage`. It should also retain the
burrowed morph identity and should not assign failure merely because the mission
ended.
