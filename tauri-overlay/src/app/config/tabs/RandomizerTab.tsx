import * as React from "react";
import type {
    AppSettings,
    LocalizedText,
    OverlayRandomizerCatalog,
    RandomizerResult,
} from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import { PreviewManager } from "../../previews/PreviewManager";
import type { PrestigeNameMap } from "../types";
import SelectionPreview from "./SelectionPreview";
import { Grid } from "@mui/material";
import styles from "../page.module.css";

type RandomizerChoices = AppSettings["rng_choices"];
type RandomizerDraft = {
    rng_choices?: RandomizerChoices | null;
};

type MutatorCatalogEntry =
    NonNullable<OverlayRandomizerCatalog>["mutators"][number];
type BrutalPlusCatalogEntry =
    NonNullable<OverlayRandomizerCatalog>["brutal_plus"][number];
type CommanderRandomizerResult = Extract<
    RandomizerResult,
    { kind: "commander" }
>;
type MutatorRandomizerResult = Extract<RandomizerResult, { kind: "mutator" }>;

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
            <div className={styles.tabContent}>
                <section
                    className={[styles.card, styles.group]
                        .filter(Boolean)
                        .join(" ")}
                >
                    <h3>{t("ui_randomizer_title")}</h3>
                    <p className={styles.note}>
                        {t("ui_randomizer_unavailable")}
                    </p>
                </section>
            </div>
        );
    }

    return (
        <div className={styles.tabContent}>
            <section
                className={[styles.card, styles.group, styles.randomizerRoot]
                    .filter(Boolean)
                    .join(" ")}
            >
                <div className={styles.randomizerLayout}>
                    <div
                        className={[
                            styles.randomizerPane,
                            styles.randomizerPaneLeft,
                        ]
                            .filter(Boolean)
                            .join(" ")}
                    >
                        <div className={styles.randomizerControls}>
                            <Grid
                                container
                                spacing={1}
                                className={styles.randomizerInlineField}
                            >
                                <Grid>
                                    <span className={styles.fieldLabel}>
                                        {t("ui_randomizer_mastery_mode")}
                                    </span>
                                </Grid>
                                <Grid>
                                    <select
                                        className={[
                                            styles.input,
                                            styles.randomizerSelect,
                                        ]
                                            .filter(Boolean)
                                            .join(" ")}
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

                                <Grid className={styles.randomizerToggle}>
                                    <input
                                        type="checkbox"
                                        checked={includeMap}
                                        onChange={(event) =>
                                            setIncludeMap(event.target.checked)
                                        }
                                    />
                                    <span>{t("ui_randomizer_random_map")}</span>
                                </Grid>

                                <Grid className={styles.randomizerToggle}>
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

                        <Grid container className={styles.randomizerMainGrid}>
                            <Grid
                                size={6}
                                className={styles.randomizerChoiceBox}
                            >
                                <h3>{t("ui_randomizer_choices_title")}</h3>
                                <div className={styles.randomizerTableShell}>
                                    <table
                                        className={[
                                            styles.dataTable,
                                            styles.randomizerChoiceTable,
                                        ]
                                            .filter(Boolean)
                                            .join(" ")}
                                    >
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
                                                            className={
                                                                styles.randomizerHeaderToggleCell
                                                            }
                                                        >
                                                            <button
                                                                type="button"
                                                                className={[
                                                                    styles.randomizerHeaderToggle,
                                                                    styles.buttonRandomizerTable,
                                                                ]
                                                                    .filter(
                                                                        Boolean,
                                                                    )
                                                                    .join(" ")}
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
                                                    <td
                                                        className={
                                                            styles.randomizerCommanderCell
                                                        }
                                                    >
                                                        <button
                                                            type="button"
                                                            className={[
                                                                styles.randomizerCommanderToggle,
                                                                styles.buttonRandomizerTable,
                                                            ]
                                                                .filter(Boolean)
                                                                .join(" ")}
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
                                                                    className={
                                                                        styles.randomizerCheckboxCell
                                                                    }
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

                                <div className={styles.randomizerActions}>
                                    <button
                                        type="button"
                                        className={styles.buttonNormal}
                                        onClick={onGenerateCommander}
                                        disabled={actions.isBusy}
                                    >
                                        {t("ui_randomizer_generate")}
                                    </button>
                                </div>
                            </Grid>

                            <Grid
                                size={6}
                                className={styles.randomizerResultBox}
                            >
                                <h3>{t("ui_randomizer_result")}</h3>
                                {commanderResult ? (
                                    <>
                                        <div
                                            className={
                                                styles.randomizerResultHead
                                            }
                                        >
                                            {`${languageManager.localize(commanderResult.commander)} - ${prestigeLabelForLanguage(
                                                catalog.prestige_names,
                                                commanderResult.commander,
                                                commanderResult.prestige,
                                                languageManager.currentLanguage(),
                                            )} (P${commanderResult.prestige})`}
                                        </div>
                                        <div
                                            className={
                                                styles.randomizerResultPreviews
                                            }
                                        >
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
                                                className={
                                                    styles.randomizerResultPreview
                                                }
                                                titleClassName={
                                                    styles.randomizerResultPreviewTitle
                                                }
                                                subtitleClassName={
                                                    styles.randomizerResultPreviewSubtitle
                                                }
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
                                                    className={
                                                        styles.randomizerResultPreview
                                                    }
                                                    titleClassName={
                                                        styles.randomizerResultPreviewTitle
                                                    }
                                                    subtitleClassName={
                                                        styles.randomizerResultPreviewSubtitle
                                                    }
                                                />
                                            ) : null}
                                        </div>
                                        <div
                                            className={[
                                                styles.statsBlock,
                                                styles.randomizerResultBody,
                                            ]
                                                .filter(Boolean)
                                                .join(" ")}
                                        >
                                            {resultMasteryRows.map(
                                                (row, index) => (
                                                    <div
                                                        key={`${row.label}-${index}`}
                                                        className={[
                                                            styles.randomizerResultRow,
                                                            row.points === 0
                                                                ? styles.isZero
                                                                : "",
                                                        ]
                                                            .filter(Boolean)
                                                            .join(" ")}
                                                    >
                                                        <span
                                                            className={
                                                                styles.randomizerResultPoints
                                                            }
                                                        >
                                                            {String(
                                                                row.points,
                                                            ).padStart(2, " ")}
                                                        </span>
                                                        <span>{` ${row.label}`}</span>
                                                    </div>
                                                ),
                                            )}
                                        </div>
                                        <div
                                            className={
                                                styles.randomizerResultFoot
                                            }
                                        >
                                            {languageManager.localizeMapRacePair(
                                                commanderResult.map_race,
                                            )}
                                        </div>
                                    </>
                                ) : (
                                    <div
                                        className={[
                                            styles.statsBlock,
                                            styles.randomizerResultBody,
                                        ]
                                            .filter(Boolean)
                                            .join(" ")}
                                    >
                                        <div
                                            className={
                                                styles.randomizerResultEmpty
                                            }
                                        >
                                            {t("ui_randomizer_empty_result")}
                                        </div>
                                    </div>
                                )}
                            </Grid>
                        </Grid>
                    </div>
                    <div
                        className={[
                            styles.randomizerPane,
                            styles.randomizerPaneRight,
                        ]
                            .filter(Boolean)
                            .join(" ")}
                    >
                        <Grid container className={styles.randomizerMainGrid}>
                            <Grid
                                size={4}
                                className={styles.randomizerChoiceBox}
                            >
                                <h3>
                                    {t("ui_randomizer_mutator_settings_title")}
                                </h3>
                                <div
                                    className={styles.randomizerMutatorSettings}
                                >
                                    <div className={styles.randomizerControls}>
                                        <Grid
                                            container
                                            className={
                                                styles.randomizerInlineField
                                            }
                                        >
                                            <Grid size={4}>
                                                <span
                                                    className={
                                                        styles.fieldLabel
                                                    }
                                                >
                                                    {t(
                                                        "ui_randomizer_mutator_mode",
                                                    )}
                                                </span>
                                            </Grid>
                                            <Grid size={8}>
                                                <select
                                                    className={[
                                                        styles.input,
                                                        styles.randomizerSelect,
                                                    ]
                                                        .filter(Boolean)
                                                        .join(" ")}
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
                                                className={
                                                    styles.randomizerRangeGroup
                                                }
                                            >
                                                <Grid
                                                    container
                                                    size={12}
                                                    className={
                                                        styles.randomizerInlineField
                                                    }
                                                >
                                                    <Grid size={4}>
                                                        <span
                                                            className={
                                                                styles.fieldLabel
                                                            }
                                                        >
                                                            {t(
                                                                "ui_common_minimum",
                                                            )}
                                                        </span>
                                                    </Grid>
                                                    <Grid size={8}>
                                                        <input
                                                            className={[
                                                                styles.input,
                                                                styles.randomizerNumberInput,
                                                            ]
                                                                .filter(Boolean)
                                                                .join(" ")}
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
                                                    className={
                                                        styles.randomizerInlineField
                                                    }
                                                >
                                                    <Grid size={4}>
                                                        <span
                                                            className={
                                                                styles.fieldLabel
                                                            }
                                                        >
                                                            {t(
                                                                "ui_common_maximum",
                                                            )}
                                                        </span>
                                                    </Grid>
                                                    <Grid size={8}>
                                                        <input
                                                            className={[
                                                                styles.input,
                                                                styles.randomizerNumberInput,
                                                            ]
                                                                .filter(Boolean)
                                                                .join(" ")}
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
                                                className={
                                                    styles.randomizerInlineField
                                                }
                                            >
                                                <Grid size={4}>
                                                    <span
                                                        className={
                                                            styles.fieldLabel
                                                        }
                                                    >
                                                        {t(
                                                            "ui_randomizer_mutator_brutal_plus",
                                                        )}
                                                    </span>
                                                </Grid>
                                                <Grid size={8}>
                                                    <select
                                                        className={[
                                                            styles.input,
                                                            styles.randomizerSelect,
                                                        ]
                                                            .filter(Boolean)
                                                            .join(" ")}
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
                                        <p className={styles.note}>
                                            {formatText(
                                                "ui_randomizer_mutator_all_random_summary",
                                                {
                                                    min: mutatorMin,
                                                    max: mutatorMax,
                                                },
                                            )}
                                        </p>
                                    ) : selectedBrutalPlusEntry ? (
                                        <div
                                            className={
                                                styles.randomizerMutatorBudget
                                            }
                                        >
                                            <div
                                                className={
                                                    styles.randomizerMutatorChip
                                                }
                                            >
                                                {`${t("ui_randomizer_mutator_count")}: ${selectedBrutalPlusEntry.mutator_count.min}-${selectedBrutalPlusEntry.mutator_count.max}`}
                                            </div>
                                            <div
                                                className={
                                                    styles.randomizerMutatorChip
                                                }
                                            >
                                                {`${t("ui_randomizer_mutator_points")}: ${selectedBrutalPlusEntry.mutator_points.min}-${selectedBrutalPlusEntry.mutator_points.max}`}
                                            </div>
                                        </div>
                                    ) : null}
                                    <p className={styles.note}>
                                        {formatText(
                                            "ui_randomizer_mutator_pool_summary",
                                            {
                                                count: catalog.mutators.length,
                                            },
                                        )}
                                    </p>
                                </div>

                                <div className={styles.randomizerActions}>
                                    <button
                                        type="button"
                                        className={styles.buttonNormal}
                                        onClick={onGenerateMutator}
                                        disabled={actions.isBusy}
                                    >
                                        {t("ui_randomizer_generate")}
                                    </button>
                                </div>
                            </Grid>

                            <Grid
                                size={8}
                                className={styles.randomizerResultBox}
                            >
                                <h3>{t("ui_randomizer_result")}</h3>
                                {mutatorResult ? (
                                    <>
                                        <div
                                            className={
                                                styles.randomizerResultHead
                                            }
                                        >
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
                                        <div
                                            className={
                                                styles.randomizerMutatorBudget
                                            }
                                        >
                                            <div
                                                className={
                                                    styles.randomizerMutatorChip
                                                }
                                            >
                                                {`${t("ui_randomizer_mutator_count")}: ${mutatorResult.mutator_count}`}
                                            </div>
                                            <div
                                                className={
                                                    styles.randomizerMutatorChip
                                                }
                                            >
                                                {`${t("ui_randomizer_mutator_total_points")}: ${mutatorResult.mutator_total_points}`}
                                            </div>
                                        </div>
                                        <div
                                            className={
                                                styles.randomizerMutatorGrid
                                            }
                                        >
                                            {mutatorResult.mutators.map(
                                                (mutator) => (
                                                    <article
                                                        key={mutator.id}
                                                        className={
                                                            styles.randomizerMutatorCard
                                                        }
                                                    >
                                                        <img
                                                            className={
                                                                styles.randomizerMutatorIcon
                                                            }
                                                            src={mutatorIconPath(
                                                                mutator.iconName,
                                                            )}
                                                            alt={localizedMutatorText(
                                                                mutator.name,
                                                                languageManager,
                                                            )}
                                                        />
                                                        <div
                                                            className={
                                                                styles.randomizerMutatorCopy
                                                            }
                                                        >
                                                            <div
                                                                className={
                                                                    styles.randomizerMutatorCardHead
                                                                }
                                                            >
                                                                <h4
                                                                    className={
                                                                        styles.randomizerMutatorName
                                                                    }
                                                                >
                                                                    {localizedMutatorText(
                                                                        mutator.name,
                                                                        languageManager,
                                                                    )}
                                                                </h4>
                                                                <span
                                                                    className={
                                                                        styles.randomizerMutatorPoints
                                                                    }
                                                                >
                                                                    {formatText(
                                                                        "ui_randomizer_mutator_point_value",
                                                                        {
                                                                            points: mutator.points,
                                                                        },
                                                                    )}
                                                                </span>
                                                            </div>
                                                            <p
                                                                className={
                                                                    styles.randomizerMutatorDescription
                                                                }
                                                            >
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
                                    <div
                                        className={[
                                            styles.statsBlock,
                                            styles.randomizerResultBody,
                                        ]
                                            .filter(Boolean)
                                            .join(" ")}
                                    >
                                        <div
                                            className={
                                                styles.randomizerResultEmpty
                                            }
                                        >
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
