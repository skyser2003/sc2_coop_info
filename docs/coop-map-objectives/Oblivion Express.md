# Oblivion Express

## Source

- Internal mission ID: `AC_TarsonisTrain`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `6ef1d8a13dfaf718f117ff859bd430202d24e09eb5bcee1cb9716a3959f8aa66`

## Primary objectives

The mission requires nine destroyed trains. Escaped trains do not advance that
count, and too many escapes cause immediate defeat.

| Difficulty band | Escapes that cause defeat | Maximum tolerated escapes |
| --- | ---: | ---: |
| First three map tiers | 3 | 2 |
| Last three map tiers | 2 | 1 |

Scheduled train events occur at approximately `05:00`, `08:00`, `11:00`, `14:00`
(double), `17:00`, `20:00` (double), `23:00`, and `25:00` (adaptive single or
double). Because misses can cause additional trains to be spawned, the overlay must
not assume exactly nine total train lifecycles.

Use the `TarsonisEngine` birth as the train identity and associate its cars by spawn
cluster/lane. A combat engine death is a destroyed train. Reaching the end region is
an escaped train; tracker data does not expose region entry directly, but its
scripted exit/removal, position, and lack of a co-op killer can support deterministic
classification when all are available.

| Aggregate outcome | Authoritative map condition |
| --- | --- |
| `completed` | Nine normal trains have been destroyed. |
| `failed` | The escape count reaches the difficulty-specific limit. |

A compact overlay should preserve every destroyed/escaped train result, rather than
silently ignoring escapes.

## Bonus objectives

Two fast bonus trains use `TarsonisEngineFast` and activate near `12:00` and
`21:00`, on the lower route in the verified revision.

- Engine combat death: that train is `completed`.
- Caboose reaching its exit: that train `failed`/escaped.
- Both destroyed: aggregate bonus `completed` (`2/2`).
- Both escaped: aggregate bonus explicitly `failed` (`0/2`).
- One destroyed and one escaped: it remains `partial` while the main mission is in
  progress, then the normal main-victory cleanup explicitly marks the aggregate
  bonus `failed` because it is not complete.
- A main defeat or interrupted replay does not run that victory cleanup; preserve
  any unfinished aggregate as `unresolved` with its observed `0/2` or `1/2`
  progress.

The existing replay heuristic also uses the fast engine's position (`x < 196`) to
exclude unrelated/placed objects. Prefer full lifecycle grouping to a position-only
rule when possible.

## Current analyzer gap

The analyzer records a `TarsonisEngineFast` death in the expected position as bonus
success without checking the complete train lifecycle. It can therefore confuse a
scripted escape removal with destruction. It also does not retain fast-train escape,
aggregate cleanup state, or main train escape state.
