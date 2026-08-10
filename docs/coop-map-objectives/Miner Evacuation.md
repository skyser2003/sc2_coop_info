# Miner Evacuation

## Source

- Internal mission ID: `AC_JarbanPointCapture`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `57670d4aaa2e45056f61f942316c5da83ab05a92155eca0fe10c62b1ee187345`

## Primary objectives

Nine `IndustrialShip` colony ships exist as possible sites. Activating one changes
it from the available mission role (player 8 in this revision) to the active target
role (player 10), starts a `120`-second holdout, and exposes its launch lifecycle.

| Difficulty | Launches required | Ship losses that cause defeat | Maximum tolerated losses |
| --- | ---: | ---: | ---: |
| Casual / Normal | 4 | 3 | 2 |
| Hard / Brutal | 5 | 2 | 1 |

This means a successful run can resolve as many as seven ship sites on either
difficulty band, which explains the useful seven-marker overlay model. It does not
mean seven successful launches are required.

| Per-ship result | Map behavior | Tracker evidence |
| --- | --- | --- |
| `launched` | The active ship survives the holdout, returns from player 10 to player 8, and is removed about 13 seconds later. | Owner change back to the available role, followed by killer-less removal. |
| `destroyed` | The active `IndustrialShip` dies. | Combat death while the lifecycle is active. |
| `unresolved` | Replay/mission end occurs during the holdout. | Neither launch return nor combat death occurred. |

Aggregate outcomes:

- Required launch count reached: both primary objectives complete, even if some
  earlier ships were lost below the limit.
- Ship-loss limit reached: both primary objectives fail immediately.

Do not count a killer-less removal after a successful launch as a destroyed ship.

## Bonus objectives

The map randomizes which of the two bosses becomes available first. Both have a
`360`-second AI-time limit that pauses while the target has been attacked recently.

### Blightbringer

- Unit: `Blightbringer`, bonus-enemy role player 5 in this revision.
- Combat death before expiry: `completed`.
- Timer expiry: `failed`; the script burrows/removes the survivor.
- Mission end before either result: `unresolved`.

### Eradicator

The Eradicator objective is one boss represented by two `NovaEradicator` units
(missile and cannon components), controlled by player 9.

- First component death is progress `1/2` only and enrages the survivor.
- Second component death completes the objective.
- Timer expiry while either component survives fails the objective and removes the
  remaining component(s).
- Mission end with one or two components alive is `unresolved`.

## Current analyzer gap

The analyzer detects Blightbringer combat death. Its current Eradicator condition
waits until one prior `NovaEradicator` loss exists, so it emits completion on the
second component death as required. It still needs an explicit shared lifecycle to
expose `0/2` and `1/2` progress, timeout failure, and unresolved state.
