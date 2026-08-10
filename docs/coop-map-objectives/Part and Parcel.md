# Part and Parcel

## Source

- Internal mission ID: `AC_PartAndParcel`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `361eacb2ae00c9c3fed0ae069251cc27865e142fcd1f2d9318c92685d81f969b`

## Primary objectives

The mission contains three rounds. Each collection round requires 70 parts, then
the Balius fights one real Hybrid boss. Destroying the third boss wins.

Parts are represented by the `PartsPickup*` family, but the authoritative progress
increment is a `PlayerEffectUsed` event (`PartsPickupSet*`). It is not a simple
tracker unit-death count. Collection progress resets to `0/70` for every round and
is hidden while the boss fight is active.

The three real bosses are `HybridDominatorCoopBoss`, controlled by player 3, at
approximately:

1. `(78.646, 135.209)`
2. `(159.232, 62.688)`
3. `(155.729, 140.311)`

Use those fixed map identities/positions to exclude illusions or other Hybrid
units. Each real boss death completes one round; boss three triggers victory.

The script defines a base of `15:45` on the lower tiers or `12:00` on Hard/Brutal,
then subtracts one boss-defeat increment before starting the first countdown. The
actual initial values are therefore `09:45` on Casual/Normal, `07:00` on Hard, and
`07:30` on Brutal. Every collected part adds one seventieth of that tier's
boss-defeat increment, so 70 parts add `06:00`, `05:00`, or `04:30` respectively.
Starting a boss fight adds another `05:00` on the lower tiers or `04:00` on
Hard/Brutal; defeating a boss adds `06:00` on the lower tiers, `05:00` on Hard, or
`04:30` on Brutal.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | Third real boss dies. |
| `failed` | The prevent-awakening timer expires. |

Exact remaining time cannot be reconstructed from tracker events alone because part
pickup progress is a game effect.

There is a map-script state-key defect worth preserving in tests: the
`PreventBossAwakenFailed` function creates/targets Primary 02 but writes the failed
state to `AC_PartAndParcel_Primary01`. Timer expiry is still deterministically a
failure of the prevent-awakening objective and the mission, even if a raw campaign
objective-state replay reports Primary 01 failed and leaves Primary 02 active.
The common base-defeat path also explicitly fails only Primary 01 in this revision.

## Bonus objectives

Two bonus trains activate at approximately `08:10` and `15:10`, including their
ten-second start delay. They use player 8, `TarsonisEngine`, and a caboose.

- Engine combat death completes one bonus train.
- Caboose reaching the exit fails/escapes that train.
- `2/2` destroyed completes the aggregate bonus.
- `0/2` after the second escape explicitly fails it.
- `1/2` remains `partial`; the script does not convert it to aggregate completion.
- A mission ending before resolution leaves remaining train state `unresolved`.

Placed normal train props at `(169, 99)` and `(38, 178)` are not bonus trains and
must be excluded. Group all engine/car deaths in one lifecycle so one train produces
only one result.

## Current analyzer gap

The analyzer deduplicates up to two `Caboose`/`TarsonisEngine` death times and
excludes the known props, but it does not require a combat engine death. A caboose
script-removed at the exit can therefore be misclassified as success. Main part
progress needs `PlayerEffectUsed` parsing; boss deaths alone only give round
completions.
