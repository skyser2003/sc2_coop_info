# Co-op Map Objective Reference

This document is the entry point for deterministic co-op objective reconstruction
from replay data. Detailed specifications live in
[`docs/coop-map-objectives/`](./coop-map-objectives/README.md).

The specifications were derived from the map archives referenced by local replays,
then cross-checked against the replay analyzer's tracker-event handling. They
distinguish the map's authoritative objective state from states that can actually be
reconstructed from tracker events.

## Common mission conditions

These conditions apply to all 15 standard co-op maps and are not repeated in each
map's primary-outcome table.

| Condition | Deterministic outcome | Scope and replay handling |
| --- | --- | --- |
| Base defeat | The mission is `failed` when all destroyable structures belonging to either allied player are destroyed. In map-script terms, that player's surviving `PreventDefeat` structure group is empty. | This determines the overall mission and primary-objective failure. It does not automatically determine bonus state; map-specific bonus cleanup still applies. |
| Incomplete replay | An active objective is `unresolved` when replay data ends before its completion or failure trigger. | Preserve every outcome already observed. A replay loss/player result of `0` alone must not convert remaining objectives to `failed`. |

No other success or failure condition is common to all 15 maps. Objective timers,
protected-unit deaths, escape or loss limits, region entry, and bonus cleanup are
map-specific.

| Map | Primary progress model | Bonus objectives | Specification |
| --- | --- | ---: | --- |
| Chain of Ascension | Four Hybrid groups, then the Rak'Shir endpoint | 2 | [Chain of Ascension](./coop-map-objectives/Chain%20of%20Ascension.md) |
| Cradle of Death | Four two-truck facility deliveries | 2 | [Cradle of Death](./coop-map-objectives/Cradle%20of%20Death.md) |
| Dead of Night | Dynamic infestation count plus day/night cycle | 1 | [Dead of Night](./coop-map-objectives/Dead%20of%20Night.md) |
| Lock & Load | Five reversible Celestial Locks | 1 | [Lock & Load](./coop-map-objectives/Lock%20%26%20Load.md) |
| Malwarfare | Four lock downloads plus final docking | 2 | [Malwarfare](./coop-map-objectives/Malwarfare.md) |
| Miner Evacuation | Four or five launches with a ship-loss limit | 2 | [Miner Evacuation](./coop-map-objectives/Miner%20Evacuation.md) |
| Mist Opportunities | Five bot waves containing eleven bots | 2 | [Mist Opportunities](./coop-map-objectives/Mist%20Opportunities.md) |
| Oblivion Express | Nine destroyed trains with a miss limit | 2 | [Oblivion Express](./coop-map-objectives/Oblivion%20Express.md) |
| Part and Parcel | Three 70-part rounds and three Hybrid bosses | 2 | [Part and Parcel](./coop-map-objectives/Part%20and%20Parcel.md) |
| Rifts to Korhal | Ten Void Shards in four timed stages | 2 | [Rifts to Korhal](./coop-map-objectives/Rifts%20to%20Korhal.md) |
| Scythe of Amon | Five Void Slivers and a shared deadline | 3 | [Scythe of Amon](./coop-map-objectives/Scythe%20of%20Amon.md) |
| Temple of the Past | Survive to 26:10.8; Thrasher kills are optional milestones | 3 | [Temple of the Past](./coop-map-objectives/Temple%20of%20the%20Past.md) |
| The Vermillion Problem | Twenty carrier drop-offs and a paused lava timer | 1 | [The Vermillion Problem](./coop-map-objectives/The%20Vermillion%20Problem.md) |
| Void Launch | Seven shuttle waves with an escape limit | 3 | [Void Launch](./coop-map-objectives/Void%20Launch.md) |
| Void Thrashing | Ten Void Thrashers in four timed groups | 1 | [Void Thrashing](./coop-map-objectives/Void%20Thrashing.md) |

Do not treat the compact progress model as the objective truth. For example,
Temple's five Thrasher encounters are useful overlay milestones, but its primary
objective is to keep the temple alive until the survival timer expires.
