import * as React from "react";
import type { LanguageManager } from "../../i18n/languageManager";
import { PreviewManager } from "../../previews/PreviewManager";
import type {
    CommanderMasteryEntry,
    CommanderMasteryMap,
    PrestigeNameMap,
} from "../types";
import SelectionPreview from "./SelectionPreview";

type RandomizerChoices = Record<string, boolean>;

type RandomizerDraft = {
    rng_choices?: RandomizerChoices | null;
};

type RandomizerCatalog = {
    prestige_names: PrestigeNameMap;
    commander_mastery: CommanderMasteryMap;
};

type RandomizerMasteryRow = {
    points: number;
    label: string;
};

type RandomizerResult = {
    commander: string;
    prestige: number;
    prestige_name: string;
    mastery: RandomizerMasteryRow[];
    map_race: string;
};

type RandomizerGeneratePayload = {
    rng_choices: RandomizerChoices;
    mastery_mode: "all_in" | "random" | "none";
    include_map: boolean;
    include_race: boolean;
};

type RandomizerTabProps = {
    draft: RandomizerDraft | null;
    catalog: RandomizerCatalog | null;
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
    value: RandomizerGeneratePayload["mastery_mode"];
}> = [
    { labelId: "ui_randomizer_mastery_all_in", value: "all_in" },
    { labelId: "ui_randomizer_mastery_random", value: "random" },
    { labelId: "ui_randomizer_mastery_none", value: "none" },
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

function masteryLabelsForLanguage(
    masteryEntry: CommanderMasteryEntry | undefined,
    language: "en" | "ko",
): string[] {
    if (!masteryEntry) {
        return [];
    }
    if (Array.isArray(masteryEntry)) {
        return masteryEntry;
    }
    return masteryEntry[language].length > 0
        ? masteryEntry[language]
        : masteryEntry.en;
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
        React.useState<RandomizerGeneratePayload["mastery_mode"]>("all_in");
    const [includeMap, setIncludeMap] = React.useState(true);
    const [includeRace, setIncludeRace] = React.useState(true);
    const [result, setResult] = React.useState<RandomizerResult | null>(null);

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
        () => previewManager.splitMapRacePair(result?.map_race || ""),
        [previewManager, result?.map_race],
    );
    const resultCommanderPreview = React.useMemo(
        () => previewManager.commander(result?.commander || ""),
        [previewManager, result?.commander],
    );
    const resultMapPreview = React.useMemo(
        () => previewManager.map(resultMapRace.map),
        [previewManager, resultMapRace.map],
    );
    const resultMasteryLabels = React.useMemo(
        () =>
            result
                ? masteryLabelsForLanguage(
                      catalog?.commander_mastery[result.commander],
                      languageManager.currentLanguage(),
                  )
                : [],
        [catalog, languageManager, result],
    );

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

    async function onGenerate() {
        const nextResult = await actions.generateRandomizer({
            rng_choices: effectiveChoices,
            mastery_mode: masteryMode,
            include_map: includeMap,
            include_race: includeRace,
        });
        if (nextResult) {
            setResult(nextResult);
        }
    }

    if (!catalog || commanderNames.length === 0) {
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
                <div className="randomizer-controls">
                    <label className="randomizer-inline-field">
                        <span className="field-label">
                            {t("ui_randomizer_mastery_mode")}
                        </span>
                        <select
                            className="input randomizer-select"
                            aria-label={t("ui_randomizer_mastery_mode_aria")}
                            value={masteryMode}
                            onChange={(event) =>
                                setMasteryMode(
                                    event.target
                                        .value as RandomizerGeneratePayload["mastery_mode"],
                                )
                            }
                        >
                            {MASTERY_MODES.map((mode) => (
                                <option key={mode.value} value={mode.value}>
                                    {t(mode.labelId)}
                                </option>
                            ))}
                        </select>
                    </label>

                    <label className="randomizer-toggle">
                        <input
                            type="checkbox"
                            checked={includeMap}
                            onChange={(event) =>
                                setIncludeMap(event.target.checked)
                            }
                        />
                        <span>{t("ui_randomizer_random_map")}</span>
                    </label>

                    <label className="randomizer-toggle">
                        <input
                            type="checkbox"
                            checked={includeRace}
                            onChange={(event) =>
                                setIncludeRace(event.target.checked)
                            }
                        />
                        <span>{t("ui_randomizer_random_enemy_race")}</span>
                    </label>
                </div>

                <div className="randomizer-main-grid">
                    <div className="randomizer-choice-box">
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
                                        {[0, 1, 2, 3].map((prestige) => (
                                            <th
                                                key={`head-${prestige}`}
                                                className="randomizer-header-toggle-cell"
                                            >
                                                <button
                                                    type="button"
                                                    className="randomizer-header-toggle"
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
                                        ))}
                                    </tr>
                                </thead>
                                <tbody>
                                    {commanderNames.map((commander) => (
                                        <tr key={commander}>
                                            <td className="randomizer-commander-cell">
                                                <button
                                                    type="button"
                                                    className="randomizer-commander-toggle"
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
                                            {[0, 1, 2, 3].map((prestige) => {
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
                                                                ] || false
                                                            }
                                                            onChange={(event) =>
                                                                setChoice(
                                                                    commander,
                                                                    prestige,
                                                                    event.target
                                                                        .checked,
                                                                )
                                                            }
                                                        />
                                                    </td>
                                                );
                                            })}
                                        </tr>
                                    ))}
                                </tbody>
                            </table>
                        </div>

                        <div className="randomizer-actions">
                            <button
                                type="button"
                                onClick={onGenerate}
                                disabled={actions.isBusy}
                            >
                                {t("ui_randomizer_generate")}
                            </button>
                        </div>
                    </div>

                    <div className="randomizer-result-box">
                        <h3>{t("ui_randomizer_result")}</h3>
                        <div className="randomizer-result-head">
                            {result
                                ? `${languageManager.localize(result.commander)} - ${prestigeLabelForLanguage(
                                      catalog.prestige_names,
                                      result.commander,
                                      result.prestige,
                                      languageManager.currentLanguage(),
                                  )} (P${result.prestige})`
                                : "-"}
                        </div>
                        {result ? (
                            <div className="randomizer-result-previews">
                                <SelectionPreview
                                    assetUrl={resultCommanderPreview.url}
                                    title={languageManager.localize(
                                        result.commander,
                                    )}
                                    subtitle={`${prestigeLabelForLanguage(
                                        catalog.prestige_names,
                                        result.commander,
                                        result.prestige,
                                        languageManager.currentLanguage(),
                                    )} (P${result.prestige})`}
                                    kind="commander"
                                    className="randomizer-result-preview"
                                    titleClassName="randomizer-result-preview-title"
                                    subtitleClassName="randomizer-result-preview-subtitle"
                                />
                                {resultMapRace.map !== "" ? (
                                    <SelectionPreview
                                        assetUrl={resultMapPreview.url}
                                        title={languageManager.localize(
                                            resultMapRace.map,
                                        )}
                                        subtitle={
                                            resultMapRace.race !== ""
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
                        ) : null}
                        <div className="stats-block randomizer-result-body">
                            {result ? (
                                result.mastery.map((row, index) => {
                                    const localizedLabel =
                                        resultMasteryLabels[index] || row.label;
                                    return (
                                        <div
                                            key={`${row.label}-${index}`}
                                            className={`randomizer-result-row${
                                                row.points === 0
                                                    ? " is-zero"
                                                    : ""
                                            }`}
                                        >
                                            <span className="randomizer-result-points">
                                                {String(row.points).padStart(
                                                    2,
                                                    " ",
                                                )}
                                            </span>
                                            <span>{` ${localizedLabel}`}</span>
                                        </div>
                                    );
                                })
                            ) : (
                                <div className="randomizer-result-empty">
                                    {t("ui_randomizer_empty_result")}
                                </div>
                            )}
                        </div>
                        <div className="randomizer-result-foot">
                            {languageManager.localizeMapRacePair(
                                result?.map_race || "",
                            )}
                        </div>
                    </div>
                </div>
            </section>
        </div>
    );
}
