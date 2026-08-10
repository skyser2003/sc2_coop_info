# Temple of the Past

## Source

- Internal mission ID: `AC_ShakurasTemple`
- Verified replay build: `96883`
- Cached map archive SHA-256:
  `71c727a48c1636eea3f652804584a468a2e0f54c8f4f13a76971486e7f781697`

## Primary objective

The only primary progression rule is to keep the fixed temple objective
(`UnitFromId(19)` in this archive) alive until the victory timer expires. The timer
is started with the map expression `26.18 * 60.0`, which equals `1570.8` seconds or
`26:10.8`; `26.18` is a decimal-minute value, not `26:18` clock notation.

| Outcome | Authoritative map condition |
| --- | --- |
| `completed` | The victory timer expires while the temple is alive. |
| `failed` | The temple dies before timer expiry. |

Temple maximum life across the map's six difficulty values is
`6000, 5000, 4000, 3000, 2000, 1000`. The fixed temple unit's death is a direct
tracker signal. Exact remaining health over time requires damage/vital tracking,
but aggregate success is established by the map victory result and timer.

## Void Thrasher pressure milestones

The two attack patterns each create five `VoidThrasher` units in four encounters:
`1 + 1 + 1 + 2`. Their combat deaths are useful overlay pressure markers. They are
not required to win: surviving to the timer with living Thrashers still completes
the primary objective.

An overlay must label these as encounter or defense milestones, never as
`Destroy 5 Void Thrashers` primary progress.

## Bonus objective

Three fixed `ZenithStone` units, controlled by player 8, comprise one aggregate
bonus objective. Each death increments progress, and the third completes it.

| Outcome | Authoritative condition |
| --- | --- |
| `completed` | All three Zenith Stones die (`3/3`). |
| `partial` | One or two die without aggregate completion. |
| `failed` | The common base-defeat condition fires while the bonus is incomplete. |
| `unresolved` | Temple-destruction defeat, normal victory with fewer than three, or interrupted replay without completion. |

The base-defeat path explicitly invokes the Zenith Stone failure trigger. The
temple-destruction path fails the primary objective but does not fail the Zenith
Stone objective in the analyzed archive.

## Current analyzer gap

The analyzer correctly recognizes `ZenithStone` deaths. It should retain `0/3` to
`3/3`, distinguish explicit base-defeat failure from other incomplete endings, and
treat Thrasher deaths only as optional main-timeline milestones.
