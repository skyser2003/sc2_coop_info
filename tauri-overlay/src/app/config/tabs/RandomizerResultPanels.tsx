import * as React from "react";
import type { OverlayRandomizerCatalog } from "../../../bindings/overlay";
import type { LanguageManager } from "../../i18n/languageManager";
import { PreviewManager } from "../../previews/PreviewManager";
import SelectionPreview from "./SelectionPreview";
import styles from "../configStyles";
import {
    brutalPlusLabel,
    localizedMutatorText,
    masteryRowsFromIndices,
    mutatorIconPath,
    prestigeLabelForLanguage,
    type CommanderRandomizerResult,
    type MutatorRandomizerResult,
} from "./randomizerModels";

type FormatText = (
    id: string,
    values?: Record<string, string | number>,
) => string;

type CommanderResultPanelProps = {
    catalog: OverlayRandomizerCatalog;
    formatText: FormatText;
    languageManager: LanguageManager;
    result: CommanderRandomizerResult | null;
};

type MutatorResultPanelProps = {
    formatText: FormatText;
    languageManager: LanguageManager;
    result: MutatorRandomizerResult | null;
};

export function CommanderResultPanel({
    catalog,
    languageManager,
    result,
}: CommanderResultPanelProps) {
    const t = (id: string) => languageManager.translate(id);
    const previewManager = React.useMemo(
        () => new PreviewManager(languageManager),
        [languageManager],
    );
    const resultMapRace = React.useMemo(
        () => previewManager.splitMapRacePair(result?.map_race || ""),
        [previewManager, result],
    );
    const resultCommanderPreview = React.useMemo(
        () => previewManager.commander(result?.commander || ""),
        [previewManager, result],
    );
    const resultMapPreview = React.useMemo(
        () => previewManager.map(resultMapRace.map),
        [previewManager, resultMapRace.map],
    );
    const resultMasteryRows = React.useMemo(
        () =>
            result
                ? masteryRowsFromIndices(
                      result.commander,
                      result.mastery_indices,
                      languageManager,
                  )
                : [],
        [languageManager, result],
    );

    if (result === null) {
        return (
            <div
                className={[styles.statsBlock, styles.randomizerResultBody]
                    .filter(Boolean)
                    .join(" ")}
            >
                <div className={styles.randomizerResultEmpty}>
                    {t("ui_randomizer_empty_result")}
                </div>
            </div>
        );
    }

    const prestigeLabel = prestigeLabelForLanguage(
        catalog.prestige_names,
        result.commander,
        result.prestige,
        languageManager.currentLanguage(),
    );

    return (
        <>
            <div className={styles.randomizerResultHead}>
                {`${languageManager.localize(result.commander)} - ${prestigeLabel} (P${result.prestige})`}
            </div>
            <div className={styles.randomizerResultPreviews}>
                <SelectionPreview
                    assetUrl={resultCommanderPreview.url}
                    title={languageManager.localize(result.commander)}
                    subtitle={`${prestigeLabel} (P${result.prestige})`}
                    kind="commander"
                    className={styles.randomizerResultPreview}
                    titleClassName={styles.randomizerResultPreviewTitle}
                    subtitleClassName={styles.randomizerResultPreviewSubtitle}
                />
                {resultMapRace.map !== "" ? (
                    <SelectionPreview
                        assetUrl={resultMapPreview.url}
                        title={languageManager.localize(resultMapRace.map)}
                        subtitle={
                            resultMapRace.race !== ""
                                ? languageManager.localize(resultMapRace.race)
                                : undefined
                        }
                        kind="map"
                        className={styles.randomizerResultPreview}
                        titleClassName={styles.randomizerResultPreviewTitle}
                        subtitleClassName={
                            styles.randomizerResultPreviewSubtitle
                        }
                    />
                ) : null}
            </div>
            <div
                className={[styles.statsBlock, styles.randomizerResultBody]
                    .filter(Boolean)
                    .join(" ")}
            >
                {resultMasteryRows.map((row, index) => (
                    <div
                        key={`${row.label}-${index}`}
                        className={[
                            styles.randomizerResultRow,
                            row.points === 0 ? styles.isZero : "",
                        ]
                            .filter(Boolean)
                            .join(" ")}
                    >
                        <span className={styles.randomizerResultPoints}>
                            {String(row.points).padStart(2, " ")}
                        </span>
                        <span>{` ${row.label}`}</span>
                    </div>
                ))}
            </div>
            <div className={styles.randomizerResultFoot}>
                {languageManager.localizeMapRacePair(result.map_race)}
            </div>
        </>
    );
}

export function MutatorResultPanel({
    formatText,
    languageManager,
    result,
}: MutatorResultPanelProps) {
    const t = (id: string) => languageManager.translate(id);

    if (result === null) {
        return (
            <div
                className={[styles.statsBlock, styles.randomizerResultBody]
                    .filter(Boolean)
                    .join(" ")}
            >
                <div className={styles.randomizerResultEmpty}>
                    {t("ui_randomizer_mutator_empty_result")}
                </div>
            </div>
        );
    }

    return (
        <>
            <div className={styles.randomizerResultHead}>
                {result.brutal_plus === null
                    ? formatText("ui_randomizer_mutator_result_head_random", {
                          count: result.mutator_count,
                      })
                    : formatText("ui_randomizer_mutator_result_head_bplus", {
                          brutalPlus: brutalPlusLabel(
                              t("difficulty_brutal_plus"),
                              result.brutal_plus,
                          ),
                      })}
            </div>
            <div className={styles.randomizerMutatorBudget}>
                <div className={styles.randomizerMutatorChip}>
                    {`${t("ui_randomizer_mutator_count")}: ${result.mutator_count}`}
                </div>
                <div className={styles.randomizerMutatorChip}>
                    {`${t("ui_randomizer_mutator_total_points")}: ${result.mutator_total_points}`}
                </div>
            </div>
            <div className={styles.randomizerMutatorGrid}>
                {result.mutators.map((mutator) => (
                    <article
                        key={mutator.id}
                        className={styles.randomizerMutatorCard}
                    >
                        <img
                            className={styles.randomizerMutatorIcon}
                            src={mutatorIconPath(mutator.iconName)}
                            alt={localizedMutatorText(
                                mutator.name,
                                languageManager,
                            )}
                        />
                        <div className={styles.randomizerMutatorCopy}>
                            <div className={styles.randomizerMutatorCardHead}>
                                <h4 className={styles.randomizerMutatorName}>
                                    {localizedMutatorText(
                                        mutator.name,
                                        languageManager,
                                    )}
                                </h4>
                                <span
                                    className={styles.randomizerMutatorPoints}
                                >
                                    {formatText(
                                        "ui_randomizer_mutator_point_value",
                                        {
                                            points: mutator.points,
                                        },
                                    )}
                                </span>
                            </div>
                            <p className={styles.randomizerMutatorDescription}>
                                {localizedMutatorText(
                                    mutator.description,
                                    languageManager,
                                )}
                            </p>
                        </div>
                    </article>
                ))}
            </div>
        </>
    );
}
