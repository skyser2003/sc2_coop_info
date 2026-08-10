# Chain of Ascension

## Source

- Internal mission ID: `AC_SlaynPayload`
- Campaign objective key prefix: `AC_SlaynHamsterBall`
- Verified replay build: `97563`
- Cached map archive SHA-256:
  `1f720523a4466ecbf65989c022f16953e8b0294626f5579050698b1184440bae`

## Primary objectives

The map exposes two required objectives: defeat Amon's champion in Rak'Shir and
keep Ji'nara alive. Four Hybrid reinforcement groups are progression encounters,
not four separate map objectives.

| Outcome | Authoritative map condition | Overlay interpretation |
| --- | --- | --- |
| `completed` | Amon's champion is pushed into the victory endpoint (region 19). | End the main timeline as victory. The champion need not die. |
| `failed` | Ji'nara or her tug-of-war bubble enters the loss endpoint (region 18). | Report Rak'Shir lost. |

The script schedules four major Hybrid pusher groups at approximately `09:00`,
`15:00`, `23:00`, and `30:00`; spatial advancement can cause their associated
logic to occur differently. A group can contain `HybridDominatorVoid` or
`HybridBehemoth` plus combinations of `HybridReaver`, `HybridDestroyer`, and
`HybridNemesis`.

For a compact overlay, a group becomes a completed pressure milestone when the
last living member of that spawned group dies. That marker does not mean the
primary objective completed: only the lane endpoint determines victory.

## Bonus objectives

There are two independent Slayn Elemental objectives:

| Elemental | Nominal activation | Map rule |
| --- | ---: | --- |
| 1 | `10:00` | Kill the first `SlaynElemental` before its timed life expires. |
| 2 | `16:00` | Kill the second `SlaynElemental`; its route is randomly selected from two variants. |

Each Elemental has six minutes of AI-time timed life. Its timer pauses while it
has been attacked recently. A combat death completes that Elemental objective;
timer expiry removes it and fails that objective. If the mission ends first, the
objective is `unresolved`.

Tracker reconstruction:

- `SlaynElemental` is controlled by the bonus role (player 10 in the verified
  revision).
- A death credited to a co-op player is deterministic success.
- A killer-less scripted removal near six unpaused minutes after activation is
  deterministic timeout failure.
- Track the two unit lifecycles independently; do not collapse them into one kill.

## Current analyzer gap

The detailed analyzer recognizes co-op-attributed `SlaynElemental` deaths. It does
not yet retain timeout failures, unresolved objectives, Rak'Shir endpoint state, or
the four Hybrid groups as stable lifecycle clusters.
