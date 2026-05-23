# Durable Thermo-Nuclear Refactor Plan

Last updated: 2026-05-24

## Milestones

- [x] Phase 0: Save this durable plan.
- [x] Phase 1: Type the stats API boundary.
- [x] Phase 2: Consolidate stats filtering and aggregation.
- [x] Phase 3: Decompose oversized orchestration.
- [x] Phase 4: Frontend and test cleanup.
- [x] Post-phase cleanup: tighten internal visibility and squash mechanical commits.

## Work Log

- 2026-05-24: Phase 1 landed for the stats API boundary. `StatsStatePayload.analysis` now uses generated Rust/TypeScript stats structs instead of `Record<string, any>`, serialized field names remain stable, and contract tests cover empty payloads plus detailed unit rows with null commander entries. Statistics rendering still uses row adapters internally; replacing those with typed view models remains in Phase 4.
- 2026-05-24: Phase 2 started with a shared `StatsQuery` for UI query parsing, in-memory replay filtering, and SQLite query derivation. Shared aggregation constants and mastery distribution bucket formatting moved into `stats_aggregation`. Remaining Phase 2 work is the larger consolidation of duplicate in-memory/SQLite aggregate row builders into one shared aggregation pipeline.
- 2026-05-24: Phase 2 continued by moving duplicated in-memory and SQLite aggregate payload row structs into `stats_aggregation`. Both paths now share the serialized row definitions for map, commander, difficulty, region, player, unit, and top-level analysis payloads.
- 2026-05-24: Phase 2 continued by moving shared ratio, median, mastery normalization, mastery distribution, prestige, and mastery row-map helpers into `stats_aggregation` for both in-memory and SQLite statistics paths.
- 2026-05-24: Phase 2 continued by moving SQLite statistics replay/player/unit snapshot structs into `stats_aggregation`, making the database statistics loader return shared typed snapshot shapes.
- 2026-05-24: Phase 2 continued by adding a typed snapshot-to-`ReplayInfo` adapter and making SQLite detailed unit statistics reuse the in-memory unit rollup builder.
- 2026-05-24: Phase 2 continued by moving the commander aggregate accumulator into `stats_aggregation` so in-memory and SQLite statistics share the same commander aggregate state shape.
- 2026-05-24: Phase 2 continued by moving the prestige date cutoff and UTC date conversion helper into `stats_aggregation`, keeping in-memory and SQLite prestige-count eligibility aligned.
- 2026-05-24: Phase 2 continued by moving the generic win/loss accumulator into `stats_aggregation` and using it for both in-memory and SQLite difficulty aggregation.
- 2026-05-24: Phase 2 continued by moving the region aggregate accumulator into `stats_aggregation`, sharing result counts, max ascension, max commander, and prestige-max recording.
- 2026-05-24: Phase 2 continued by moving commander player recording and commander totals into `stats_aggregation`, removing duplicated mastery, prestige, APM, and kill-fraction update logic.
- 2026-05-24: Phase 2 continued by moving commander row construction into `stats_aggregation`, so main and ally commander payloads share one frequency, mastery, prestige, and `any` row builder.
- 2026-05-24: Phase 2 continued by moving player aggregate recording into `stats_aggregation`, sharing player result, APM, kill-fraction, commander-frequency, alias, and last-seen tracking.
- 2026-05-24: Phase 2 continued by moving map aggregate recording into `stats_aggregation`, sharing detailed counts, win/loss counts, bonus rates, average victory time, and fastest-map tracking while preserving existing tie-date behavior.
- 2026-05-24: Phase 2 completed with shared stats query parsing, typed SQLite replay snapshots, shared stats payload row contracts, shared unit rollups, and shared map, commander, player, region, difficulty, mastery, prestige, and ratio aggregation helpers.
- 2026-05-24: Phase 3 started by extracting the stats Tauri commands from `lib.rs` into `commands::stats`, while keeping the registered command names stable.
- 2026-05-24: Phase 3 continued by extracting replay page/show/chat/visual/move Tauri commands into `commands::replays`, including the replay page request model.
- 2026-05-24: Phase 3 continued by extracting core config get/update/action Tauri commands into `commands::config`, while keeping command names and payload behavior stable.
- 2026-05-24: Phase 3 continued by moving config players and weeklies tab data commands into `commands::config`, keeping query defaults and database-backed loading behavior unchanged.
- 2026-05-24: Phase 3 continued by moving generic system/UI Tauri commands into `commands::system`, leaving helper implementations unchanged.
- 2026-05-24: Phase 3 continued by moving startup warmup and auto-update helpers into `services::startup`.
- 2026-05-24: Phase 3 continued by moving tray state, tray setup, and clean exit handling into `services::tray`.
- 2026-05-24: Phase 3 continued by moving startup window preparation into `services::windows`.
- 2026-05-24: Phase 3 continued by moving window close policy into `services::windows` while preserving the public `WindowCloseAction` export.
- 2026-05-24: Phase 3 continued by moving first-win bonus scan and timer orchestration into `services::today_win_bonus`, keeping existing `TauriOverlayOps` call sites stable.
- 2026-05-24: Phase 3 continued by moving replay watcher notification, retry parsing, detailed-cache persistence, and replay-show detailed parsing into `services::replay_watcher`.
- 2026-05-24: Phase 3 continued by moving live-game launch polling and delayed player-stats popup orchestration into `services::game_launch`.
- 2026-05-24: Phase 3 continued by moving stats progress events, startup analysis requests, detailed cache generation, cache-entry merging, and background analysis threads into `services::analysis`.
- 2026-05-24: Phase 3 continued by moving `ReplayInfo` and `ReplayPlayerInfo` implementations into `replay_info`, keeping the replay model with its struct definitions.
- 2026-05-24: Phase 3 continued by moving stats query-string parsing into `stats_query`, removing root-level query parser helpers from `lib.rs`.
- 2026-05-24: Phase 3 continued by moving replay cache-entry conversion plus replay chat and visual payload construction into `services::replay_payload`.
- 2026-05-24: Phase 3 continued by moving analysis mode/status helpers, detailed progress parsing, empty stats payload construction, rebuild snapshot application, and detailed-cache status syncing into `stats_state`.
- 2026-05-24: Phase 3 continued by moving Tauri menu events, window events, and setup orchestration into `services::app_lifecycle`, leaving `run()` focused on builder registration.
- 2026-05-24: Phase 3 continued by moving config runtime setting application into `commands::config`, keeping config update side effects with the command path that owns them.
- 2026-05-24: Phase 3 continued by moving commander and Amon unit stats payload builders into `stats_units`, keeping `lib.rs` under 800 lines and avoiding a new oversized `stats_aggregation` module.
- 2026-05-24: Phase 3 continued by moving remaining root helper implementations into focused payload, system, replay, and stats ops modules. `lib.rs` now contains module declarations, exports, constants, and `run()`.
- 2026-05-24: Phase 3 continued by moving replay scan progress and selected replay state out of `backend_state`, keeping those fields private in focused state modules and reducing `backend_state.rs` below 1k lines.
- 2026-05-24: Phase 3 completed with Tauri command registration, app lifecycle setup, replay watching, launch polling, startup analysis, stats state helpers, replay payload builders, root helper ops, and replay state/progress split out of the crate root and oversized backend state.
- 2026-05-24: Phase 4 started by extracting shared Playwright config/Tauri mocks into `tests/helpers/config-mock.ts` and switching duplicated games-filter and pagination specs onto the shared helper.
- 2026-05-24: Phase 4 continued by extracting config route Tauri request wrappers, hotkey capture state, and games/players/weeklies tab data loading into focused frontend modules and hooks. Config route tests now use local timestamp expectations and robust monitor-select targeting.
- 2026-05-24: Phase 4 continued by moving statistics filter/query state, refresh scheduling, analysis event subscriptions, and stats action handlers into `useConfigStats`.
- 2026-05-24: Phase 4 continued by moving settings load/save, live apply queuing, draft replacement, theme preview, and settings status handling into `useConfigSettings`.
- 2026-05-24: Phase 4 continued by extracting the statistics filter/action controls into `StatisticsFiltersPanel`, removing the bulky filter grid from the main statistics tab component.
- 2026-05-24: Phase 4 continued by extracting the difficulty/region statistics renderer into a typed panel with shared table-header and view-model helpers.
- 2026-05-24: Phase 4 continued by extracting the Amon unit statistics table into a typed panel backed by generated unit row bindings.
- 2026-05-24: Phase 4 continued by extracting the player unit statistics renderer into a typed panel with a local generated-row guard for commander unit rows.
- 2026-05-24: Phase 4 continued by extracting the map statistics and fastest-replay details into a typed panel backed by generated map/fastest payload rows.
- 2026-05-24: Phase 4 continued by extracting commander tables, commander details, and mastery distribution charts into `StatisticsCommandersPanel`, leaving the main statistics tab as subtab orchestration.
- 2026-05-24: Phase 4 continued by removing the frontend `JsonObject` overlay from `StatisticsAnalysis` and switching commander statistics rendering to generated commander row bindings.
- 2026-05-24: Phase 4 completed with config route hooks, shared Playwright/Tauri config mocks, typed statistics view-model helpers, generated stats row consumption, and split statistics panels.
- 2026-05-24: Post-phase cleanup removed remaining `pub(super)` from Rust source, narrowed DB sibling visibility to `pub(in crate::db)`, made stats aggregate row/input fields private behind constructors and accessors, and made the startup analysis outcome fields private.
- 2026-05-24: Final post-phase cleanup removed remaining restricted Rust visibility markers, moved standalone helper functions under owner structs, and kept only thin module-scope Tauri command adapters where the macro requires them.

## Summary

Refactor in phases, starting with the highest-leverage stats contract cleanup, then consolidating duplicate backend stats logic, then decomposing the oversized backend/frontend modules. Preserve behavior and keep every step compiling and tested on Windows, macOS, and Linux assumptions.

## Key Changes

### Phase 0: Save The Plan

- Add `docs/thermo-nuclear-refactor-plan.md`.
- Include this plan, milestone checkboxes, verification commands, and a short last updated section.
- Do not touch DB version.

### Phase 1: Type The Stats API Boundary

- Replace `StatsStatePayload.analysis?: Record<string, any> | null` with TS-exported Rust structs for the stats payload shape.
- Add typed rows for map, commander, ally commander, difficulty, region, player, unit, fastest-map details, mastery distribution, and prestige labels.
- Update frontend config/statistics code to consume generated types instead of `JsonObject`, broad casts, and defensive row readers.
- Keep serialized field names stable, including existing names such as `MapData`, `CommanderData`, `UnitData`, `Victory`, `Defeat`, and `Winrate`.

### Phase 2: Consolidate Stats Filtering And Aggregation

- Promote one canonical `StatsQuery` or filter model shared by in-memory and SQLite-backed paths.
- Move duplicated constants and aggregate row builders into one stats aggregation module.
- Make SQLite loading return typed replay snapshots, then apply the same aggregator used by non-SQL paths.
- Preserve current query behavior for wins/losses, date bounds, length bounds, difficulty exclusions, region exclusions, multibox filtering, current replay selection, and detailed-only unit stats.

### Phase 3: Decompose Oversized Orchestration

- Reduce Tauri `lib.rs` to exports, command registration, and `run()`.
- Move config, replay, and stats commands into command modules; move startup analysis, replay watching, and update/tray setup into focused services.
- Replace exposed `Arc<Mutex<_>>` handles with `BackendState` methods for atomic domain operations where practical.

### Phase 4: Frontend And Test Cleanup

- Split the config route into hooks for settings load/save, live apply, hotkey capture, tab data loading, and stats actions.
- Split statistics rendering into typed view-model builders plus smaller tab components.
- Extract large Playwright/Tauri test fixtures and mocks into reusable test helpers.
- After all phases land, run a final visibility and history cleanup pass: remove unnecessary `pub(crate)` or `pub(super)`, make struct fields private where module ownership allows it, and squash mechanical move/share commits into a small set of common commits.

## Public Interfaces And Compatibility

- Keep Tauri command names and frontend call sites compatible unless a generated type name changes only at compile time.
- Keep JSON wire shape compatible for existing frontend tests and snapshots.
- Generated TypeScript bindings must not contain `any` for stats analysis after Phase 1.
- Add named unit/stat structs at boundaries; avoid tuple or opaque JSON contracts except where serde compatibility requires an explicit adapter.

## Verification Commands

Before runtime commands, load `.env` or `.envrc` if present.

- Rust format: `cargo fmt`
- Rust lint: `cargo clippy --release --workspace --all-targets -- -D warnings`
- Rust tests: `cargo test --release --workspace`
- Frontend format: `npm run format`
- Frontend typecheck: `npm run typecheck`
- Frontend test typecheck: `npm run typecheck:tests`
- Frontend targeted tests: `npm run test:config -- <target spec>`

Cargo build CPU should be limited to half the available cores.

## Assumptions

- The implementation proceeds as a phased refactor, not a single all-hotspots rewrite.
- Behavior preservation is mandatory; structural cleanup is the goal.
- DB version remains unchanged unless a later deployment-specific task explicitly requires it.
