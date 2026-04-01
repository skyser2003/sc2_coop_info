import * as React from "react";
import type { OverlayRandomizerCatalog } from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import { PreviewManager } from "../../previews/PreviewManager";
import type { PrestigeNameMap } from "../types";
import SelectionPreview from "./SelectionPreview";
import { Grid } from "@mui/material";

type RandomizerChoices = Record<string, boolean>;
type RandomizerDraft = {
    rng_choices?: RandomizerChoices | null;
};

type LocalizedText = {
    en: string;
    ko: string;
};

type MutatorCatalogEntry =
    NonNullable<OverlayRandomizerCatalog>["mutators"][number];
type BrutalPlusCatalogEntry =
    NonNullable<OverlayRandomizerCatalog>["brutal_plus"][number];

type CommanderRandomizerResult = {
    kind: "commander";
    commander: string;
    prestige: number;
    mastery_indices: Array<number | null>;
    map_race: string;
};

type MutatorRandomizerResult = {
    kind: "mutator";
    mutators: MutatorCatalogEntry[];
    mutator_total_points: number;
    mutator_count: number;
    brutal_plus: number | null;
};

type RandomizerResult = CommanderRandomizerResult | MutatorRandomizerResult;

type CommanderGeneratePayload = {
    mode: "commander";
    rng_choices: RandomizerChoices;
    mastery_mode: "all_in" | "random" | "none";
    include_map: boolean;
    include_race: boolean;
};

type MutatorGeneratePayload = {
    mode: "mutator";
    mutator_mode: "all_random" | "brutal_plus";
    mutator_min: number;
    mutator_max: number;
    brutal_plus: number;
};

type RandomizerGeneratePayload =
    | CommanderGeneratePayload
    | MutatorGeneratePayload;

type RandomizerTabProps = {
    draft: RandomizerDraft | null;
    catalog: OverlayRandomizerCatalog | null;
    onChange: (path: string[], value: RandomizerChoices) => void;
    languageManager: LanguageManager;
    actions: {
        isBusy: boolean;
        generateRandomizer: (
            payload: RandomizerGeneratePayload,
        ) => Promise<RandomizerResult | null>;
    };
};

const MASTERY_MODES: Array<{
    labelId: string;
    value: CommanderGeneratePayload["mastery_mode"];
}> = [
    { labelId: "ui_randomizer_mastery_all_in", value: "all_in" },
    { labelId: "ui_randomizer_mastery_random", value: "random" },
    { labelId: "ui_randomizer_mastery_none", value: "none" },
];

const MUTATOR_MODES: Array<{
    labelId: string;
    value: MutatorGeneratePayload["mutator_mode"];
}> = [
    {
        labelId: "ui_randomizer_mutator_mode_all_random",
        value: "all_random",
    },
    {
        labelId: "ui_randomizer_mutator_mode_brutal_plus",
        value: "brutal_plus",
    },
];

function buildEffectiveChoices(
    savedChoices: RandomizerChoices | null | undefined,
    commanderNames: string[],
): RandomizerChoices {
    const hasSavedChoices =
        savedChoices !== null &&
        savedChoices !== undefined &&
        Object.keys(savedChoices).length > 0;
    const nextChoices: RandomizerChoices = {};

    for (const commander of commanderNames) {
        for (let prestige = 0; prestige <= 3; prestige += 1) {
            const key = `${commander}_${prestige}`;
            nextChoices[key] = hasSavedChoices
                ? Boolean(savedChoices[key])
                : prestige === 0;
        }
    }

    return nextChoices;
}

function areAllCommanderPrestigesSelected(
    choices: RandomizerChoices,
    commander: string,
): boolean {
    for (let prestige = 0; prestige <= 3; prestige += 1) {
        if (!choices[`${commander}_${prestige}`]) {
            return false;
        }
    }
    return true;
}

function areAllPrestigeColumnSelected(
    choices: RandomizerChoices,
    commanderNames: string[],
    prestige: number,
): boolean {
    return commanderNames.every(
        (commander) => choices[`${commander}_${prestige}`] === true,
    );
}

function prestigeLabelForLanguage(
    prestigeNames: PrestigeNameMap,
    commander: string,
    prestige: number,
    language: "en" | "ko",
): string {
    const localized = prestigeNames[commander];
    if (!localized) {
        return `P${prestige}`;
    }

    return (
        localized[language]?.[prestige] ||
        localized.en?.[prestige] ||
        `P${prestige}`
    );
}

function masteryRowsFromIndices(
    commander: string,
    masteryIndices: Array<number | null>,
    languageManager: LanguageManager,
): Array<{ points: number; label: string }> {
    const labels = languageManager.commanderMasteryLabels(commander);
    const rows: Array<{ points: number; label: string }> = [];

    for (let pairIndex = 0; pairIndex < 3; pairIndex += 1) {
        const selected = masteryIndices[pairIndex];
        const leftIndex = pairIndex * 2;
        const rightIndex = leftIndex + 1;
        const leftPoints =
            selected === null || selected === undefined ? 0 : selected;
        const rightPoints =
            selected === null || selected === undefined ? 0 : 30 - selected;

        rows.push({
            points: leftPoints,
            label: labels[leftIndex] || `Mastery ${leftIndex + 1}`,
        });
        rows.push({
            points: rightPoints,
            label: labels[rightIndex] || `Mastery ${rightIndex + 1}`,
        });
    }

    return rows;
}

function clampNumber(value: number, min: number, max: number): number {
    if (!Number.isFinite(value)) {
        return min;
    }
    return Math.min(max, Math.max(min, value));
}

function mutatorIconPath(iconName: string): string {
    return `/overlay/Mutator Icons/${encodeURIComponent(iconName)}.png`;
}

function localizedMutatorText(
    value: LocalizedText,
    languageManager: LanguageManager,
): string {
    return languageManager.currentLanguage() === "ko"
        ? value.ko || value.en
        : value.en || value.ko;
}

function brutalPlusLabel(brutalPlusText: string, level: number): string {
    return `${brutalPlusText}${level}`;
}

export default function RandomizerTab({
    draft,
    catalog,
    onChange,
    languageManager,
    actions,
}: RandomizerTabProps) {
    const t = (id: string) => languageManager.translate(id);
    const formatText = (
        id: string,
        values: Record<string, string | number> = {},
    ): string =>
        Object.entries(values).reduce(
            (text, [key, value]) =>
                text.split(`{{${key}}}`).join(String(value)),
            t(id),
        );
    const [masteryMode, setMasteryMode] =
        React.useState<CommanderGeneratePayload["mastery_mode"]>("all_in");
    const [includeMap, setIncludeMap] = React.useState(true);
    const [includeRace, setIncludeRace] = React.useState(true);
    const [mutatorMode, setMutatorMode] =
        React.useState<MutatorGeneratePayload["mutator_mode"]>("all_random");
    const [mutatorMin, setMutatorMin] = React.useState<number>(1);
    const [mutatorMax, setMutatorMax] = React.useState<number>(3);
    const [mutatorMinInput, setMutatorMinInput] = React.useState<string>("1");
    const [mutatorMaxInput, setMutatorMaxInput] = React.useState<string>("3");
    const [selectedBrutalPlus, setSelectedBrutalPlus] =
        React.useState<number>(1);
    const [commanderResult, setCommanderResult] =
        React.useState<CommanderRandomizerResult | null>(null);
    const [mutatorResult, setMutatorResult] =
        React.useState<MutatorRandomizerResult | null>(null);

    const commanderNames = React.useMemo(
        () => Object.keys(catalog?.prestige_names || {}),
        [catalog],
    );
    const effectiveChoices = React.useMemo(
        () => buildEffectiveChoices(draft?.rng_choices, commanderNames),
        [draft, commanderNames],
    );
    const previewManager = React.useMemo(
        () => new PreviewManager(languageManager),
        [languageManager],
    );
    const resultMapRace = React.useMemo(
        () => previewManager.splitMapRacePair(commanderResult?.map_race || ""),
        [commanderResult, previewManager],
    );
    const resultCommanderPreview = React.useMemo(
        () => previewManager.commander(commanderResult?.commander || ""),
        [commanderResult, previewManager],
    );
    const resultMapPreview = React.useMemo(
        () => previewManager.map(resultMapRace.map),
        [previewManager, resultMapRace.map],
    );
    const resultMasteryRows = React.useMemo(
        () =>
            commanderResult
                ? masteryRowsFromIndices(
                      commanderResult.commander,
                      commanderResult.mastery_indices,
                      languageManager,
                  )
                : [],
        [commanderResult, languageManager],
    );
    const brutalPlusEntries = React.useMemo(
        () => catalog?.brutal_plus || [],
        [catalog],
    );
    const selectedBrutalPlusEntry = React.useMemo(
        () =>
            brutalPlusEntries.find(
                (entry) => entry.brutal_plus === selectedBrutalPlus,
            ) || brutalPlusEntries[0],
        [brutalPlusEntries, selectedBrutalPlus],
    );

    React.useEffect(() => {
        if (selectedBrutalPlusEntry) {
            setSelectedBrutalPlus(selectedBrutalPlusEntry.brutal_plus);
        }
    }, [selectedBrutalPlusEntry]);

    React.useEffect(() => {
        setMutatorMinInput(String(mutatorMin));
    }, [mutatorMin]);

    React.useEffect(() => {
        setMutatorMaxInput(String(mutatorMax));
    }, [mutatorMax]);

    function setChoice(commander: string, prestige: number, checked: boolean) {
        const nextChoices = {
            ...effectiveChoices,
            [`${commander}_${prestige}`]: checked,
        };
        onChange(["rng_choices"], nextChoices);
    }

    function toggleCommander(commander: string) {
        const shouldSelect = !areAllCommanderPrestigesSelected(
            effectiveChoices,
            commander,
        );
        const nextChoices = { ...effectiveChoices };
        for (let prestige = 0; prestige <= 3; prestige += 1) {
            nextChoices[`${commander}_${prestige}`] = shouldSelect;
        }
        onChange(["rng_choices"], nextChoices);
    }

    function togglePrestigeColumn(prestige: number) {
        const shouldSelect = !areAllPrestigeColumnSelected(
            effectiveChoices,
            commanderNames,
            prestige,
        );
        const nextChoices = { ...effectiveChoices };
        for (const commander of commanderNames) {
            nextChoices[`${commander}_${prestige}`] = shouldSelect;
        }
        onChange(["rng_choices"], nextChoices);
    }

    function displayCommanderName(commander: string): string {
        return languageManager.localize(commander);
    }

    function commitMutatorMin(rawValue: string) {
        const nextMin = clampNumber(Number(rawValue), 1, 10);
        setMutatorMin(nextMin);
        setMutatorMax((current) => Math.max(current, nextMin));
    }

    function commitMutatorMax(rawValue: string) {
        const nextMax = clampNumber(Number(rawValue), 1, 10);
        setMutatorMax(nextMax);
        setMutatorMin((current) => Math.min(current, nextMax));
    }

    function maybeCommitMutatorInput(
        event: React.KeyboardEvent<HTMLInputElement>,
        commit: (rawValue: string) => void,
    ) {
        if (event.key === "Enter") {
            commit(event.currentTarget.value);
            event.currentTarget.blur();
        }
    }

    async function onGenerateCommander() {
        const nextResult = await actions.generateRandomizer({
            mode: "commander",
            rng_choices: effectiveChoices,
            mastery_mode: masteryMode,
            include_map: includeMap,
            include_race: includeRace,
        });
        if (nextResult?.kind === "commander") {
            setCommanderResult(nextResult);
        }
    }

    async function onGenerateMutator() {
        const nextResult = await actions.generateRandomizer({
            mode: "mutator",
            mutator_mode: mutatorMode,
            mutator_min: mutatorMin,
            mutator_max: mutatorMax,
            brutal_plus: selectedBrutalPlusEntry?.brutal_plus || 1,
        });
        if (nextResult?.kind === "mutator") {
            setMutatorResult(nextResult);
        }
    }

    if (
        !catalog ||
        (commanderNames.length === 0 && catalog.mutators.length === 0)
    ) {
        return (
            <div className="tab-content">
                <section className="card group">
                    <h3>{t("ui_randomizer_title")}</h3>
                    <p className="note">{t("ui_randomizer_unavailable")}</p>
                </section>
            </div>
        );
    }

    return (
        <div className="tab-content">
            <section className="card group randomizer-root">
                <div className="randomizer-layout">
                    <div className="randomizer-pane randomizer-pane-left">
                        <div className="randomizer-controls">
                            <Grid
                                container
                                spacing={1}
                                className="randomizer-inline-field"
                            >
                                <Grid>
                                    <span className="field-label">
                                        {t("ui_randomizer_mastery_mode")}
                                    </span>
                                </Grid>
                                <Grid>
                                    <select
                                        className="input randomizer-select"
                                        aria-label={t(
                                            "ui_randomizer_mastery_mode_aria",
                                        )}
                                        value={masteryMode}
                                        onChange={(event) =>
                                            setMasteryMode(
                                                event.target
                                                    .value as CommanderGeneratePayload["mastery_mode"],
                                            )
                                        }
                                    >
                                        {MASTERY_MODES.map((mode) => (
                                            <option
                                                key={mode.value}
                                                value={mode.value}
                                            >
                                                {t(mode.labelId)}
                                            </option>
                                        ))}
                                    </select>
                                </Grid>

                                <Grid className="randomizer-toggle">
                                    <input
                                        type="checkbox"
                                        checked={includeMap}
                                        onChange={(event) =>
                                            setIncludeMap(event.target.checked)
                                        }
                                    />
                                    <span>{t("ui_randomizer_random_map")}</span>
                                </Grid>

                                <Grid className="randomizer-toggle">
                                    <input
                                        type="checkbox"
                                        checked={includeRace}
                                        onChange={(event) =>
                                            setIncludeRace(event.target.checked)
                                        }
                                    />
                                    <span>
                                        {t("ui_randomizer_random_enemy_race")}
                                    </span>
                                </Grid>
                            </Grid>
                        </div>

                        <Grid container className="randomizer-main-grid">
                            <Grid size={6} className="randomizer-choice-box">
                                <h3>{t("ui_randomizer_choices_title")}</h3>
                                <div className="randomizer-table-shell">
                                    <table className="data-table randomizer-choice-table">
                                        <thead>
                                            <tr>
                                                <th>
                                                    {t(
                                                        "ui_randomizer_commander_column",
                                                    )}
                                                </th>
                                                {[0, 1, 2, 3].map(
                                                    (prestige) => (
                                                        <th
                                                            key={`head-${prestige}`}
                                                            className="randomizer-header-toggle-cell"
                                                        >
                                                            <button
                                                                type="button"
                                                                className="randomizer-header-toggle button-randomizer-table"
                                                                aria-label={formatText(
                                                                    "ui_randomizer_toggle_prestige_all",
                                                                    {
                                                                        prestige,
                                                                    },
                                                                )}
                                                                onClick={() =>
                                                                    togglePrestigeColumn(
                                                                        prestige,
                                                                    )
                                                                }
                                                            >
                                                                {`P${prestige}`}
                                                            </button>
                                                        </th>
                                                    ),
                                                )}
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {commanderNames.map((commander) => (
                                                <tr key={commander}>
                                                    <td className="randomizer-commander-cell">
                                                        <button
                                                            type="button"
                                                            className="randomizer-commander-toggle button-randomizer-table"
                                                            aria-label={formatText(
                                                                "ui_randomizer_toggle_all_prestiges",
                                                                {
                                                                    commander:
                                                                        displayCommanderName(
                                                                            commander,
                                                                        ),
                                                                },
                                                            )}
                                                            onClick={() =>
                                                                toggleCommander(
                                                                    commander,
                                                                )
                                                            }
                                                        >
                                                            {displayCommanderName(
                                                                commander,
                                                            )}
                                                        </button>
                                                    </td>
                                                    {[0, 1, 2, 3].map(
                                                        (prestige) => {
                                                            const prestigeLabel =
                                                                prestigeLabelForLanguage(
                                                                    catalog.prestige_names,
                                                                    commander,
                                                                    prestige,
                                                                    languageManager.currentLanguage(),
                                                                );
                                                            return (
                                                                <td
                                                                    key={`${commander}-${prestige}`}
                                                                    className="randomizer-checkbox-cell"
                                                                >
                                                                    <input
                                                                        type="checkbox"
                                                                        aria-label={`${commander} P${prestige}`}
                                                                        title={
                                                                            prestigeLabel
                                                                        }
                                                                        checked={
                                                                            effectiveChoices[
                                                                                `${commander}_${prestige}`
                                                                            ] ||
                                                                            false
                                                                        }
                                                                        onChange={(
                                                                            event,
                                                                        ) =>
                                                                            setChoice(
                                                                                commander,
                                                                                prestige,
                                                                                event
                                                                                    .target
                                                                                    .checked,
                                                                            )
                                                                        }
                                                                    />
                                                                </td>
                                                            );
                                                        },
                                                    )}
                                                </tr>
                                            ))}
                                        </tbody>
                                    </table>
                                </div>

                                <div className="randomizer-actions">
                                    <button
                                        type="button"
                                        className="button-normal"
                                        onClick={onGenerateCommander}
                                        disabled={actions.isBusy}
                                    >
                                        {t("ui_randomizer_generate")}
                                    </button>
                                </div>
                            </Grid>

                            <Grid size={6} className="randomizer-result-box">
                                <h3>{t("ui_randomizer_result")}</h3>
                                {commanderResult ? (
                                    <>
                                        <div className="randomizer-result-head">
                                            {`${languageManager.localize(commanderResult.commander)} - ${prestigeLabelForLanguage(
                                                catalog.prestige_names,
                                                commanderResult.commander,
                                                commanderResult.prestige,
                                                languageManager.currentLanguage(),
                                            )} (P${commanderResult.prestige})`}
                                        </div>
                                        <div className="randomizer-result-previews">
                                            <SelectionPreview
                                                assetUrl={
                                                    resultCommanderPreview.url
                                                }
                                                title={languageManager.localize(
                                                    commanderResult.commander,
                                                )}
                                                subtitle={`${prestigeLabelForLanguage(
                                                    catalog.prestige_names,
                                                    commanderResult.commander,
                                                    commanderResult.prestige,
                                                    languageManager.currentLanguage(),
                                                )} (P${commanderResult.prestige})`}
                                                kind="commander"
                                                className="randomizer-result-preview"
                                                titleClassName="randomizer-result-preview-title"
                                                subtitleClassName="randomizer-result-preview-subtitle"
                                            />
                                            {resultMapRace.map !== "" ? (
                                                <SelectionPreview
                                                    assetUrl={
                                                        resultMapPreview.url
                                                    }
                                                    title={languageManager.localize(
                                                        resultMapRace.map,
                                                    )}
                                                    subtitle={
                                                        resultMapRace.race !==
                                                        ""
                                                            ? languageManager.localize(
                                                                  resultMapRace.race,
                                                              )
                                                            : undefined
                                                    }
                                                    kind="map"
                                                    className="randomizer-result-preview"
                                                    titleClassName="randomizer-result-preview-title"
                                                    subtitleClassName="randomizer-result-preview-subtitle"
                                                />
                                            ) : null}
                                        </div>
                                        <div className="stats-block randomizer-result-body">
                                            {resultMasteryRows.map(
                                                (row, index) => (
                                                    <div
                                                        key={`${row.label}-${index}`}
                                                        className={`randomizer-result-row${
                                                            row.points === 0
                                                                ? " is-zero"
                                                                : ""
                                                        }`}
                                                    >
                                                        <span className="randomizer-result-points">
                                                            {String(
                                                                row.points,
                                                            ).padStart(2, " ")}
                                                        </span>
                                                        <span>{` ${row.label}`}</span>
                                                    </div>
                                                ),
                                            )}
                                        </div>
                                        <div className="randomizer-result-foot">
                                            {languageManager.localizeMapRacePair(
                                                commanderResult.map_race,
                                            )}
                                        </div>
                                    </>
                                ) : (
                                    <div className="stats-block randomizer-result-body">
                                        <div className="randomizer-result-empty">
                                            {t("ui_randomizer_empty_result")}
                                        </div>
                                    </div>
                                )}
                            </Grid>
                        </Grid>
                    </div>
                    <div className="randomizer-pane randomizer-pane-right">
                        <Grid container className="randomizer-main-grid">
                            <Grid size={4} className="randomizer-choice-box">
                                <h3>
                                    {t("ui_randomizer_mutator_settings_title")}
                                </h3>
                                <div className="randomizer-mutator-settings">
                                    <div className="randomizer-controls">
                                        <Grid
                                            container
                                            className="randomizer-inline-field"
                                        >
                                            <Grid size={4}>
                                                <span className="field-label">
                                                    {t(
                                                        "ui_randomizer_mutator_mode",
                                                    )}
                                                </span>
                                            </Grid>
                                            <Grid size={8}>
                                                <select
                                                    className="input randomizer-select"
                                                    aria-label={t(
                                                        "ui_randomizer_mutator_mode_aria",
                                                    )}
                                                    value={mutatorMode}
                                                    onChange={(event) =>
                                                        setMutatorMode(
                                                            event.target
                                                                .value as MutatorGeneratePayload["mutator_mode"],
                                                        )
                                                    }
                                                >
                                                    {MUTATOR_MODES.map(
                                                        (mode) => (
                                                            <option
                                                                key={mode.value}
                                                                value={
                                                                    mode.value
                                                                }
                                                            >
                                                                {t(
                                                                    mode.labelId,
                                                                )}
                                                            </option>
                                                        ),
                                                    )}
                                                </select>
                                            </Grid>
                                        </Grid>

                                        {mutatorMode === "all_random" ? (
                                            <Grid
                                                container
                                                rowSpacing={1}
                                                className="randomizer-range-group"
                                            >
                                                <Grid
                                                    container
                                                    size={12}
                                                    className="randomizer-inline-field"
                                                >
                                                    <Grid size={4}>
                                                        <span className="field-label">
                                                            {t(
                                                                "ui_common_minimum",
                                                            )}
                                                        </span>
                                                    </Grid>
                                                    <Grid size={8}>
                                                        <input
                                                            className="input randomizer-number-input"
                                                            type="number"
                                                            min={1}
                                                            max={10}
                                                            value={
                                                                mutatorMinInput
                                                            }
                                                            aria-label={t(
                                                                "ui_randomizer_mutator_min_aria",
                                                            )}
                                                            onChange={(event) =>
                                                                setMutatorMinInput(
                                                                    event.target
                                                                        .value,
                                                                )
                                                            }
                                                            onBlur={(event) =>
                                                                commitMutatorMin(
                                                                    event.target
                                                                        .value,
                                                                )
                                                            }
                                                            onKeyDown={(
                                                                event,
                                                            ) =>
                                                                maybeCommitMutatorInput(
                                                                    event,
                                                                    commitMutatorMin,
                                                                )
                                                            }
                                                        />
                                                    </Grid>
                                                </Grid>
                                                <Grid
                                                    container
                                                    size={12}
                                                    className="randomizer-inline-field"
                                                >
                                                    <Grid size={4}>
                                                        <span className="field-label">
                                                            {t(
                                                                "ui_common_maximum",
                                                            )}
                                                        </span>
                                                    </Grid>
                                                    <Grid size={8}>
                                                        <input
                                                            className="input randomizer-number-input"
                                                            type="number"
                                                            min={1}
                                                            max={10}
                                                            value={
                                                                mutatorMaxInput
                                                            }
                                                            aria-label={t(
                                                                "ui_randomizer_mutator_max_aria",
                                                            )}
                                                            onChange={(event) =>
                                                                setMutatorMaxInput(
                                                                    event.target
                                                                        .value,
                                                                )
                                                            }
                                                            onBlur={(event) =>
                                                                commitMutatorMax(
                                                                    event.target
                                                                        .value,
                                                                )
                                                            }
                                                            onKeyDown={(
                                                                event,
                                                            ) =>
                                                                maybeCommitMutatorInput(
                                                                    event,
                                                                    commitMutatorMax,
                                                                )
                                                            }
                                                        />
                                                    </Grid>
                                                </Grid>
                                            </Grid>
                                        ) : (
                                            <Grid
                                                container
                                                className="randomizer-inline-field"
                                            >
                                                <Grid size={4}>
                                                    <span className="field-label">
                                                        {t(
                                                            "ui_randomizer_mutator_brutal_plus",
                                                        )}
                                                    </span>
                                                </Grid>
                                                <Grid size={8}>
                                                    <select
                                                        className="input randomizer-select"
                                                        aria-label={t(
                                                            "ui_randomizer_mutator_brutal_plus_aria",
                                                        )}
                                                        value={
                                                            selectedBrutalPlus
                                                        }
                                                        onChange={(event) =>
                                                            setSelectedBrutalPlus(
                                                                Number(
                                                                    event.target
                                                                        .value,
                                                                ),
                                                            )
                                                        }
                                                    >
                                                        {brutalPlusEntries.map(
                                                            (entry) => (
                                                                <option
                                                                    key={
                                                                        entry.brutal_plus
                                                                    }
                                                                    value={
                                                                        entry.brutal_plus
                                                                    }
                                                                >
                                                                    {brutalPlusLabel(
                                                                        t(
                                                                            "difficulty_brutal_plus",
                                                                        ),
                                                                        entry.brutal_plus,
                                                                    )}
                                                                </option>
                                                            ),
                                                        )}
                                                    </select>
                                                </Grid>
                                            </Grid>
                                        )}
                                    </div>

                                    {mutatorMode === "all_random" ? (
                                        <p className="note">
                                            {formatText(
                                                "ui_randomizer_mutator_all_random_summary",
                                                {
                                                    min: mutatorMin,
                                                    max: mutatorMax,
                                                },
                                            )}
                                        </p>
                                    ) : selectedBrutalPlusEntry ? (
                                        <div className="randomizer-mutator-budget">
                                            <div className="randomizer-mutator-chip">
                                                {`${t("ui_randomizer_mutator_count")}: ${selectedBrutalPlusEntry.mutator_count.min}-${selectedBrutalPlusEntry.mutator_count.max}`}
                                            </div>
                                            <div className="randomizer-mutator-chip">
                                                {`${t("ui_randomizer_mutator_points")}: ${selectedBrutalPlusEntry.mutator_points.min}-${selectedBrutalPlusEntry.mutator_points.max}`}
                                            </div>
                                        </div>
                                    ) : null}
                                    <p className="note">
                                        {formatText(
                                            "ui_randomizer_mutator_pool_summary",
                                            {
                                                count: catalog.mutators.length,
                                            },
                                        )}
                                    </p>
                                </div>

                                <div className="randomizer-actions">
                                    <button
                                        type="button"
                                        className="button-normal"
                                        onClick={onGenerateMutator}
                                        disabled={actions.isBusy}
                                    >
                                        {t("ui_randomizer_generate")}
                                    </button>
                                </div>
                            </Grid>

                            <Grid size={8} className="randomizer-result-box">
                                <h3>{t("ui_randomizer_result")}</h3>
                                {mutatorResult ? (
                                    <>
                                        <div className="randomizer-result-head">
                                            {mutatorResult.brutal_plus === null
                                                ? formatText(
                                                      "ui_randomizer_mutator_result_head_random",
                                                      {
                                                          count: mutatorResult.mutator_count,
                                                      },
                                                  )
                                                : formatText(
                                                      "ui_randomizer_mutator_result_head_bplus",
                                                      {
                                                          brutalPlus:
                                                              brutalPlusLabel(
                                                                  t(
                                                                      "difficulty_brutal_plus",
                                                                  ),
                                                                  mutatorResult.brutal_plus,
                                                              ),
                                                      },
                                                  )}
                                        </div>
                                        <div className="randomizer-mutator-budget">
                                            <div className="randomizer-mutator-chip">
                                                {`${t("ui_randomizer_mutator_count")}: ${mutatorResult.mutator_count}`}
                                            </div>
                                            <div className="randomizer-mutator-chip">
                                                {`${t("ui_randomizer_mutator_total_points")}: ${mutatorResult.mutator_total_points}`}
                                            </div>
                                        </div>
                                        <div className="randomizer-mutator-grid">
                                            {mutatorResult.mutators.map(
                                                (mutator) => (
                                                    <article
                                                        key={mutator.id}
                                                        className="randomizer-mutator-card"
                                                    >
                                                        <img
                                                            className="randomizer-mutator-icon"
                                                            src={mutatorIconPath(
                                                                mutator.iconName,
                                                            )}
                                                            alt={localizedMutatorText(
                                                                mutator.name,
                                                                languageManager,
                                                            )}
                                                        />
                                                        <div className="randomizer-mutator-copy">
                                                            <div className="randomizer-mutator-card-head">
                                                                <h4 className="randomizer-mutator-name">
                                                                    {localizedMutatorText(
                                                                        mutator.name,
                                                                        languageManager,
                                                                    )}
                                                                </h4>
                                                                <span className="randomizer-mutator-points">
                                                                    {formatText(
                                                                        "ui_randomizer_mutator_point_value",
                                                                        {
                                                                            points: mutator.points,
                                                                        },
                                                                    )}
                                                                </span>
                                                            </div>
                                                            <p className="randomizer-mutator-description">
                                                                {localizedMutatorText(
                                                                    mutator.description,
                                                                    languageManager,
                                                                )}
                                                            </p>
                                                        </div>
                                                    </article>
                                                ),
                                            )}
                                        </div>
                                    </>
                                ) : (
                                    <div className="stats-block randomizer-result-body">
                                        <div className="randomizer-result-empty">
                                            {t(
                                                "ui_randomizer_mutator_empty_result",
                                            )}
                                        </div>
                                    </div>
                                )}
                            </Grid>
                        </Grid>
                    </div>
                </div>
            </section>
        </div>
    );
}
