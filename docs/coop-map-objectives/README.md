# Replay Objective Specifications

These files describe all primary and bonus objective outcomes for the standard
StarCraft II co-op mission maps. Their purpose is to support deterministic replay
overlay output, not to act as player strategy guides.

## State vocabulary

Use the following states consistently:

| State | Meaning |
| --- | --- |
| `active` | The map has activated the objective and no terminal rule has fired. |
| `completed` | The map's own completion condition has fired. |
| `failed` | The map's own failure condition has fired. |
| `partial` | Some independently countable sub-objectives completed, but the aggregate objective did not complete. |
| `mixed` | A repeatable main phase resolved with both desired and undesired outcomes, such as a Void Launch wave with destroyed and escaped shuttles. |
| `unresolved` | The replay or mission ended before the map assigned completion or failure. This is not failure. |

An overlay may use `destroyed`, `escaped`, `launched`, or similar event labels for
phase markers, but the aggregate objective state must follow the map trigger.

## Reconstruction rules

1. Map-script state is authoritative. A tracker signal is only evidence for that
   state.
2. Resolve player roles from replay slots or map identity. Numeric controller IDs
   in these files describe the verified map revision and should not be a universal
   protocol assumption.
3. Group repeated units by their lifecycle and spawn cluster before using nominal
   times. Game time, AI time, and tracker loops can differ slightly.
4. A scripted `UnitRemove` can surface as a killer-less death. Do not automatically
   classify every `SUnitDiedEvent` as combat destruction.
5. Morph events preserve an objective unit's lifecycle. They are frequently more
   reliable than unit names at one instant.
6. A replay result of loss or player result `0` does not by itself fail every bonus.
   Preserve bonuses as `unresolved` unless the map fired a failure rule or the
   objective's own failure is reconstructible.
7. If a required trigger uses regions, behaviors, orders, effects, or private
   counters not present in tracker events, report the limitation. Do not invent a
   deterministic tracker-only proxy.

## Source scope

The `Source` section in each file records the internal mission ID, replay build, and
cached map archive hash used for this analysis. Nominal timings are map-script
schedule values. Mutators, custom difficulty variants, future map revisions, and
replay corruption can change observed timings or make a state unavailable.

## Maps

- [Chain of Ascension](./Chain%20of%20Ascension.md)
- [Cradle of Death](./Cradle%20of%20Death.md)
- [Dead of Night](./Dead%20of%20Night.md)
- [Lock & Load](./Lock%20%26%20Load.md)
- [Malwarfare](./Malwarfare.md)
- [Miner Evacuation](./Miner%20Evacuation.md)
- [Mist Opportunities](./Mist%20Opportunities.md)
- [Oblivion Express](./Oblivion%20Express.md)
- [Part and Parcel](./Part%20and%20Parcel.md)
- [Rifts to Korhal](./Rifts%20to%20Korhal.md)
- [Scythe of Amon](./Scythe%20of%20Amon.md)
- [Temple of the Past](./Temple%20of%20the%20Past.md)
- [The Vermillion Problem](./The%20Vermillion%20Problem.md)
- [Void Launch](./Void%20Launch.md)
- [Void Thrashing](./Void%20Thrashing.md)
