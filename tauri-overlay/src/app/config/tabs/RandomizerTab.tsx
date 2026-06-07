import * as React from "react";
import type {
    OverlayRandomizerCatalog,
    RandomizerResult,
} from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import { Grid } from "@mui/material";
import styles from "../configStyles";
import {
    CommanderResultPanel,
    MutatorResultPanel,
} from "./RandomizerResultPanels";
import {
    MASTERY_MODES,
    MUTATOR_MODES,
    areAllCommanderPrestigesSelected,
    areAllPrestigeColumnSelected,
    brutalPlusLabel,
    buildEffectiveChoices,
    clampNumber,
    prestigeLabelForLanguage,
    type CommanderGeneratePayload,
    type CommanderRandomizerResult,
    type MutatorGeneratePayload,
    type MutatorRandomizerResult,
    type RandomizerChoices,
    type RandomizerDraft,
    type RandomizerGeneratePayload,
} from "./randomizerModels";

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
                                <CommanderResultPanel
                                    catalog={catalog}
                                    formatText={formatText}
                                    languageManager={languageManager}
                                    result={commanderResult}
                                />
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
                                <MutatorResultPanel
                                    formatText={formatText}
                                    languageManager={languageManager}
                                    result={mutatorResult}
                                />
                            </Grid>
                        </Grid>
                    </div>
                </div>
            </section>
        </div>
    );
}
