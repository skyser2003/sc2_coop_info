import { Fragment, ReactNode, useEffect, useMemo, useState } from "react";
import {
    type CommanderMasteryData,
    LanguageManager,
} from "../../i18n/languageManager";
import type { OverlayReplayPayload } from "../../../bindings/overlay";

const showmutators = true;
const maxUnits = 5;
const minimum_kills = 1;
const show_player_total_kills = false;
const assetBase = "/overlay/";

const mutatorDescriptions: Record<string, string> = {};

const bonusNumbers: Record<string, number> = {
    "Chain of Ascension": 2,
    "Cradle of Death": 2,
    "Dead of Night": 1,
    "Lock & Load": 1,
    Malwarfare: 2,
    "Miner Evacuation": 2,
    "Mist Opportunities": 2,
    "Oblivion Express": 2,
    "Part and Parcel": 2,
    "Rifts to Korhal": 2,
    "Scythe of Amon": 3,
    "Temple of the Past": 3,
    "The Vermillion Problem": 1,
    "Void Launch": 3,
    "Void Thrashing": 1,
};

type LocalizableValue = string | number | boolean | null | undefined;
type OverlayPrestigeNameCatalog = Record<
    string,
    { en: string[]; ko: string[] }
>;
type IconPayload = OverlayReplayPayload["mainIcons"];
type UnitStatsMap = OverlayReplayPayload["mainUnits"];

type UnitRow = {
    key: string;
    percent: number;
    kills: number;
    created: number;
    died: number;
    backgroundWidth: number;
    spacerClassName: string;
};

type MasteryRow = {
    value: number;
    label: string;
    className: string;
};

type CommanderSection = {
    idPrefix: "CM1" | "CM2" | "CM3";
    name: string;
    icons: ReactNode[];
    prestige: string;
    prestigeColor: string | null;
    masteryRows: MasteryRow[];
    masteryColor: string | null;
    unitRows: UnitRow[];
    unitColor: string;
};

type KillBarState = {
    mainWidth: string;
    allyWidth: string;
};

function overlayAssetPath(path: string): string {
    return `${assetBase}${path}`;
}

function readNumber(value: LocalizableValue, fallback = 0): number {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : fallback;
}

function readBoolean(value: boolean | null | undefined): boolean {
    return value === true;
}

function readString(value: LocalizableValue): string {
    return typeof value === "string"
        ? value
        : value == null
          ? ""
          : String(value);
}

function readNumericArray(
    value: readonly number[] | null | undefined,
): number[] {
    return Array.isArray(value) ? value.map((entry) => readNumber(entry)) : [];
}

function formatLength(seconds: number, multiply = true): string {
    const gameSeconds = multiply
        ? Math.round(seconds * 1.4)
        : Math.round(seconds);
    const sec = gameSeconds % 60;
    const min = ((gameSeconds - sec) / 60) % 60;
    const hr = (gameSeconds - sec - min * 60) / 3600;
    const hrPrefix = hr > 0 ? `${hr}:` : "";
    const minPart = min === 0 ? "00:" : min < 10 ? `0${min}:` : `${min}:`;
    const secPart = sec < 10 ? `0${sec}` : `${sec}`;
    return `${hrPrefix}${minPart}${secPart}`;
}

function buildCommanderLabel(
    position: "left" | "right",
    commander: LocalizableValue,
    commanderLevel: number | null | undefined,
    localize: (value: LocalizableValue) => string,
): string {
    const localizedCommander = localize(commander);
    if (localizedCommander === "") {
        return "";
    }

    const level = readNumber(commanderLevel);
    const addition = level < 15 ? `{${level}}` : "";

    return position === "left"
        ? `${localizedCommander} ${addition}`.trim()
        : `${addition} ${localizedCommander}`.trim();
}

function buildMasteryRows(values: number[], labels: string[]): MasteryRow[] {
    return values.map((value, index) => ({
        value,
        label: labels[index] ?? "",
        className:
            value === 0 ? "nomastery" : value < 10 ? "singlemastery" : "",
    }));
}

function buildUnitRows(
    unitMap: UnitStatsMap,
    commanderKey: string,
    totalKills: number,
    localizeUnitName: (value: LocalizableValue) => string,
): UnitRow[] {
    const sortedRows = Object.entries(unitMap).sort((left, right) => {
        const leftKills = readNumber(left[1]?.[2]);
        const rightKills = readNumber(right[1]?.[2]);
        if (rightKills !== leftKills) {
            return rightKills - leftKills;
        }
        const leftCreated = readNumber(left[1]?.[0]);
        const rightCreated = readNumber(right[1]?.[0]);
        if (rightCreated !== leftCreated) {
            return rightCreated - leftCreated;
        }
        return left[0].localeCompare(right[0]);
    });

    const nextRows: UnitRow[] = [];

    for (const [unitName, stats] of sortedRows) {
        if (nextRows.length === maxUnits) {
            break;
        }

        const created = readNumber(stats?.[0]);
        const died = readNumber(stats?.[1]);
        const kills = readNumber(stats?.[2]);
        const killShare = readNumber(stats?.[3]);

        if (kills < minimum_kills) {
            continue;
        }

        let displayName = unitName;
        if (displayName === "Stalker" && commanderKey === "Alarak") {
            displayName = "Slayer";
        }
        if (displayName === "Sentinel" && commanderKey === "Fenix") {
            displayName = "Legionnaire";
        }

        const percent = Math.round(100 * killShare);
        const spacerClassName =
            percent < 10
                ? "killpadding"
                : percent === 100
                  ? "nokillpadding"
                  : "";
        const backgroundWidth =
            totalKills > 0 ? (50 * kills) / totalKills : (35 * percent) / 100;

        nextRows.push({
            key: localizeUnitName(displayName),
            percent,
            kills,
            created,
            died,
            backgroundWidth,
            spacerClassName,
        });
    }

    return nextRows;
}

function buildIconNodes(iconPayload: IconPayload): ReactNode[] {
    return Object.entries(iconPayload).flatMap(([key, value]) => {
        if (key === "outlaws" && Array.isArray(value)) {
            return value.map((outlawName, index) => (
                <img
                    key={`${key}-${outlawName}-${index}`}
                    src={overlayAssetPath(`Icons/${outlawName}.png`)}
                    alt=""
                />
            ));
        }

        const count = typeof value === "number" ? value : 0;
        if (count <= 0) {
            return [];
        }

        const textClassName =
            key === "killbots"
                ? "icontext killbotkills"
                : "icontext iconcreated";
        const textPrefix = key === "killbots" ? "-" : "+";
        const label =
            [
                "hfts",
                "tus",
                "propagators",
                "voidrifts",
                "turkey",
                "voidreanimators",
                "deadofnight",
                "minesweeper",
                "missilecommand",
            ].includes(key) && key !== "killbots"
                ? String(count)
                : `${textPrefix}${count}`;

        return [
            <Fragment key={`icon-${key}`}>
                <img src={overlayAssetPath(`Icons/${key}.png`)} alt="" />
                <span className={textClassName}>{label}</span>
            </Fragment>,
        ];
    });
}

function renderMasteryRows(masteryRows: MasteryRow[]): ReactNode {
    if (masteryRows.every((row) => row.value === 0)) {
        return null;
    }

    return masteryRows.map((row, index) => (
        <span
            key={`mastery-${index}`}
            className={row.className === "" ? undefined : row.className}
        >
            {row.value} {row.label}
            <br />
        </span>
    ));
}

function renderUnitRows(
    unitRows: UnitRow[],
    color: string,
    killsLabel: string,
): ReactNode {
    if (unitRows.length === 0) {
        return <span className="unitkills" />;
    }

    return (
        <>
            <span className="unitkills">
                &nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;{killsLabel}
            </span>
            <span className="unitcreated header">created</span>
            <span className="unitdied header">lost</span>
            <br />
            {unitRows.map((row) => (
                <Fragment key={row.key}>
                    <div
                        className="unitkillbg"
                        style={{
                            width: `${row.backgroundWidth}vh`,
                            backgroundColor: color,
                        }}
                    />
                    <div className="unitline">
                        {row.key}{" "}
                        <span
                            className={`unitkills ${row.spacerClassName}`.trim()}
                        >
                            {row.percent}% | {row.kills}
                        </span>
                        <span className="unitcreated">{row.created}</span>
                        <span className="unitdied">{row.died}</span>
                    </div>
                </Fragment>
            ))}
        </>
    );
}

function prestigeLabelForLanguage(
    prestigeNames: OverlayPrestigeNameCatalog,
    commander: string,
    prestige: number,
    language: "en" | "ko",
): string {
    const localized = prestigeNames[commander];
    if (localized == null) {
        return `P${prestige}`;
    }

    return (
        localized[language]?.[prestige] ??
        localized.en?.[prestige] ??
        `P${prestige}`
    );
}

function formatPrestigeDisplay(
    prestigeName: string,
    prestigeIndex: number,
    prestigeLabel: string,
): string {
    return `${prestigeName} (${prestigeLabel} ${prestigeIndex})`;
}

export default function GameStatText({
    payload,
    replayModeVisible,
    showSessionStats,
    sessionVictoryCount,
    sessionDefeatCount,
    p1Color,
    p2Color,
    amonColor,
    masteryColor,
    cancelReplayDisplayClearTimer,
    overlayCommanderMasteryCatalog,
    overlayPrestigeNameCatalog,
    language,
    overlayLanguageManager,
    reportOverlayReplayDataState,
}: {
    payload: OverlayReplayPayload | null;
    replayModeVisible: boolean;
    showSessionStats: boolean;
    sessionVictoryCount: number;
    sessionDefeatCount: number;
    p1Color: string | null;
    p2Color: string | null;
    amonColor: string | null;
    masteryColor: string | null;
    cancelReplayDisplayClearTimer: () => void;
    overlayCommanderMasteryCatalog: CommanderMasteryData;
    overlayPrestigeNameCatalog: OverlayPrestigeNameCatalog;
    language: string;
    overlayLanguageManager: LanguageManager;
    reportOverlayReplayDataState: (active: boolean) => void;
}) {
    const overlayText = (id: string): string =>
        overlayLanguageManager.translate(id);
    const overlayLocalize = (value: LocalizableValue): string =>
        overlayLanguageManager.localize(value);
    const overlayEnglish = (value: LocalizableValue): string =>
        overlayLanguageManager.englishLabel(value);
    const statsPayload = payload;

    useEffect(() => {
        if (statsPayload != null) {
            cancelReplayDisplayClearTimer();
            reportOverlayReplayDataState(true);
            return;
        }

        reportOverlayReplayDataState(false);
    }, [
        cancelReplayDisplayClearTimer,
        reportOverlayReplayDataState,
        statsPayload,
    ]);

    const totalKills = useMemo(
        () =>
            statsPayload == null
                ? 0
                : readNumber(statsPayload.mainkills) +
                  readNumber(statsPayload.allykills),
        [statsPayload],
    );

    const targetKillBarState = useMemo<KillBarState>(() => {
        if (statsPayload == null || totalKills <= 0) {
            return {
                mainWidth: "50%",
                allyWidth: "50%",
            };
        }

        return {
            mainWidth: `${Math.round((100 * readNumber(statsPayload.mainkills)) / totalKills)}%`,
            allyWidth: `${Math.round((100 * readNumber(statsPayload.allykills)) / totalKills)}%`,
        };
    }, [statsPayload, totalKills]);

    const [killBarState, setKillBarState] = useState<KillBarState>({
        mainWidth: "50%",
        allyWidth: "50%",
    });

    useEffect(() => {
        setKillBarState({
            mainWidth: "50%",
            allyWidth: "50%",
        });

        if (
            targetKillBarState.mainWidth === "50%" &&
            targetKillBarState.allyWidth === "50%"
        ) {
            return;
        }

        const timer = setTimeout(() => {
            setKillBarState(targetKillBarState);
        }, 700);

        return () => {
            clearTimeout(timer);
        };
    }, [targetKillBarState]);

    const masteryLabelsFor = (commander: LocalizableValue): string[] => {
        const commanderKey = overlayEnglish(commander);
        if (commanderKey === "") {
            return [];
        }

        const localized = overlayCommanderMasteryCatalog[commanderKey];
        if (localized == null) {
            return [];
        }

        const currentLabels =
            localized[overlayLanguageManager.currentLanguage()];
        if (Array.isArray(currentLabels) && currentLabels.length > 0) {
            return currentLabels;
        }

        return Array.isArray(localized.en) ? localized.en : [];
    };

    const localizePrestige = (
        commander: LocalizableValue,
        prestigeValue: LocalizableValue,
    ): string => {
        const rawPrestige = readString(prestigeValue).trim();
        const commanderKey = overlayEnglish(commander);
        if (commanderKey === "") {
            return rawPrestige;
        }

        const localized = overlayPrestigeNameCatalog[commanderKey];
        if (localized == null) {
            return rawPrestige;
        }

        if (rawPrestige === "") {
            const localizedPrestigeName = prestigeLabelForLanguage(
                overlayPrestigeNameCatalog,
                commanderKey,
                0,
                language === "ko" ? "ko" : "en",
            );
            return formatPrestigeDisplay(
                localizedPrestigeName,
                0,
                overlayText("ui_stats_prestige_label"),
            );
        }

        const prestigeIndex = localized.en.findIndex((label, index) => {
            const koreanLabel = localized.ko[index] ?? "";
            return label === rawPrestige || koreanLabel === rawPrestige;
        });

        if (prestigeIndex === -1) {
            return rawPrestige;
        }

        const localizedPrestigeName = prestigeLabelForLanguage(
            overlayPrestigeNameCatalog,
            commanderKey,
            prestigeIndex,
            language === "ko" ? "ko" : "en",
        );
        return formatPrestigeDisplay(
            localizedPrestigeName,
            prestigeIndex,
            overlayText("ui_stats_prestige_label"),
        );
    };

    const viewModel = useMemo(() => {
        if (statsPayload == null) {
            return null;
        }

        const mutators = statsPayload.mutators;
        const bonus = statsPayload.bonus;
        const localizedMapName = overlayLocalize(statsPayload.map_name);
        const englishMapName = overlayEnglish(statsPayload.map_name);
        const localizedResult = overlayLocalize(statsPayload.result);
        const mainCommanderImage =
            readString(statsPayload.mainCommander) === ""
                ? ""
                : overlayAssetPath(
                      `Commanders/${overlayEnglish(statsPayload.mainCommander)}.png`,
                  );
        const allyCommanderImage =
            readString(statsPayload.allyCommander) === ""
                ? ""
                : overlayAssetPath(
                      `Commanders/${overlayEnglish(statsPayload.allyCommander)}.png`,
                  );
        const bonusTotalValue =
            statsPayload.bonus_total != null
                ? readNumber(statsPayload.bonus_total, -1)
                : -1;
        const bonusTotal =
            bonusTotalValue >= 0
                ? bonusTotalValue
                : (bonusNumbers[readString(statsPayload.map_name)] ??
                  bonusNumbers[englishMapName] ??
                  null);
        const bonusText = `(${bonus.length}/${bonusTotal ?? "?"})`;
        const percent1 =
            totalKills > 0
                ? `${Math.round((100 * readNumber(statsPayload.mainkills)) / totalKills)}%`
                : "0%";
        const percent2 =
            totalKills > 0
                ? `${Math.round((100 * readNumber(statsPayload.allykills)) / totalKills)}%`
                : "0%";
        const displayPercent1 = show_player_total_kills
            ? `${percent1} (${readNumber(statsPayload.mainkills)})`
            : percent1;
        const displayPercent2 = show_player_total_kills
            ? `${percent2} (${readNumber(statsPayload.allykills)})`
            : percent2;

        const mainCommanderKey = overlayEnglish(statsPayload.mainCommander);
        const allyCommanderKey = overlayEnglish(statsPayload.allyCommander);

        const commanderSections: CommanderSection[] = [
            {
                idPrefix: "CM1",
                name: readString(statsPayload.main),
                icons: buildIconNodes(statsPayload.mainIcons),
                prestige: localizePrestige(
                    statsPayload.mainCommander,
                    statsPayload.mainPrestige,
                ),
                prestigeColor: p1Color,
                masteryRows: buildMasteryRows(
                    readNumericArray(statsPayload.mainMasteries),
                    masteryLabelsFor(statsPayload.mainCommander),
                ),
                masteryColor,
                unitRows: buildUnitRows(
                    statsPayload.mainUnits,
                    mainCommanderKey,
                    totalKills,
                    overlayLanguageManager.localizeUnitName.bind(
                        overlayLanguageManager,
                    ),
                ),
                unitColor: p1Color ?? "#666",
            },
            {
                idPrefix: "CM2",
                name: readString(statsPayload.ally),
                icons: buildIconNodes(statsPayload.allyIcons),
                prestige: localizePrestige(
                    statsPayload.allyCommander,
                    statsPayload.allyPrestige,
                ),
                prestigeColor: p2Color,
                masteryRows: buildMasteryRows(
                    readNumericArray(statsPayload.allyMasteries),
                    masteryLabelsFor(statsPayload.allyCommander),
                ),
                masteryColor,
                unitRows: buildUnitRows(
                    statsPayload.allyUnits,
                    allyCommanderKey,
                    totalKills,
                    overlayLanguageManager.localizeUnitName.bind(
                        overlayLanguageManager,
                    ),
                ),
                unitColor: p2Color ?? "#444",
            },
            {
                idPrefix: "CM3",
                name: overlayLocalize("ui_settings_amon"),
                icons: [],
                prestige: overlayLocalize(statsPayload.comp),
                prestigeColor: amonColor,
                masteryRows: [],
                masteryColor: null,
                unitRows: buildUnitRows(
                    statsPayload.amon_units,
                    "",
                    totalKills,
                    overlayLanguageManager.localizeUnitName.bind(
                        overlayLanguageManager,
                    ),
                ),
                unitColor: "red",
            },
        ];

        const difficultyText = readBoolean(statsPayload.weekly)
            ? `${overlayText("ui_overlay_weekly")} (${overlayLocalize(statsPayload.difficulty)})`
            : readNumber(statsPayload.extension) > 0 && mutators.length > 0
              ? `${overlayText("ui_overlay_custom")} (${overlayLocalize(statsPayload.difficulty)})`
              : readNumber(statsPayload["B+"]) > 0
                ? overlayLocalize(`B+${readNumber(statsPayload["B+"])}`)
                : overlayLocalize(statsPayload.difficulty);

        const sessionText = showSessionStats
            ? `${overlayText("ui_overlay_session")}: ${sessionVictoryCount} ${overlayText("ui_overlay_wins")}/${sessionVictoryCount + sessionDefeatCount} ${overlayText("ui_overlay_games")}`
            : "";

        const randomizerText =
            statsPayload.Commander != null
                ? `${overlayText("ui_overlay_randomized_commander")}: ${overlayLocalize(statsPayload.Commander)} (${localizePrestige(statsPayload.Commander, statsPayload.Prestige)})`
                : "";

        return {
            mutators,
            localizedResult,
            mainCommanderImage,
            allyCommanderImage,
            bonusText,
            localizedMapName,
            mainName: readString(statsPayload.main),
            allyName: readString(statsPayload.ally),
            mainCommanderLabel: buildCommanderLabel(
                "left",
                statsPayload.mainCommander,
                statsPayload.mainCommanderLevel,
                overlayLocalize,
            ),
            allyCommanderLabel: buildCommanderLabel(
                "right",
                statsPayload.allyCommander,
                statsPayload.allyCommanderLevel,
                overlayLocalize,
            ),
            mainAPM: `${readNumber(statsPayload.mainAPM)} APM`,
            allyAPM: `${readNumber(statsPayload.allyAPM)} APM`,
            showRecord: readBoolean(statsPayload.fastest),
            sessionText,
            randomizerText,
            difficultyText,
            displayPercent1,
            displayPercent2,
            mainKillBarColor: totalKills > 0 ? (p1Color ?? "#666") : "#666",
            allyKillBarColor: totalKills > 0 ? (p2Color ?? "#444") : "#444",
            commanderSections,
            hasMutators: mutators.length > 0,
            showReplaySections: true,
            lengthText: formatLength(readNumber(statsPayload.length)),
        };
    }, [
        amonColor,
        masteryColor,
        overlayCommanderMasteryCatalog,
        overlayEnglish,
        overlayLanguageManager,
        overlayLocalize,
        overlayPrestigeNameCatalog,
        overlayText,
        p1Color,
        p2Color,
        statsPayload,
        totalKills,
    ]);

    const noDataText = overlayText("ui_overlay_no_data");
    const bestTimeText = overlayText("ui_overlay_best_time");
    const killsLabel = overlayText("ui_overlay_kills");
    const fallbackSessionText = showSessionStats
        ? `${overlayText("ui_overlay_session")}: ${sessionVictoryCount} ${overlayText("ui_overlay_wins")}/${sessionVictoryCount + sessionDefeatCount} ${overlayText("ui_overlay_games")}`
        : "";

    return (
        <>
            <div id="stats">
                <div id="topstats">
                    <div id="mutators">
                        {viewModel?.hasMutators ? (
                            viewModel.mutators.map((mutator, index) => (
                                <img
                                    key={`mutator-${index}-${readString(mutator)}`}
                                    src={overlayAssetPath(
                                        `Mutator Icons/${overlayEnglish(mutator)}.png`,
                                    )}
                                    alt=""
                                />
                            ))
                        ) : viewModel != null ? (
                            <span id="resultsp">
                                {viewModel.localizedResult}!
                            </span>
                        ) : null}
                    </div>
                    <div
                        id="nodata"
                        style={{
                            display: statsPayload == null ? "block" : "none",
                        }}
                    >
                        {noDataText}
                    </div>
                    <span id="name1" style={{ color: p1Color ?? undefined }}>
                        {viewModel?.mainName ?? ""}
                    </span>
                    <span id="name2" style={{ color: p2Color ?? undefined }}>
                        {viewModel?.allyName ?? ""}
                    </span>
                    <div
                        id="killbar"
                        style={{
                            display: viewModel?.showReplaySections
                                ? "block"
                                : "none",
                        }}
                    >
                        <div
                            id="killbar1"
                            style={{
                                width: killBarState.mainWidth,
                                backgroundColor:
                                    viewModel?.mainKillBarColor ?? "#666",
                            }}
                        >
                            {viewModel?.mainCommanderImage ? (
                                <img
                                    id="killbar1img"
                                    src={viewModel.mainCommanderImage}
                                    alt=""
                                />
                            ) : null}
                            <span id="percent1">
                                {viewModel?.displayPercent1 ?? ""}
                            </span>
                        </div>
                        <div
                            id="killbar2"
                            style={{
                                width: killBarState.allyWidth,
                                backgroundColor:
                                    viewModel?.allyKillBarColor ?? "#444",
                            }}
                        >
                            {viewModel?.allyCommanderImage ? (
                                <img
                                    id="killbar2img"
                                    src={viewModel.allyCommanderImage}
                                    alt=""
                                />
                            ) : null}
                            <span id="percent2">
                                {viewModel?.displayPercent2 ?? ""}
                            </span>
                        </div>
                        <div id="result">
                            {viewModel?.hasMutators
                                ? `${viewModel.localizedResult}!`
                                : overlayText("ui_overlay_kills")}
                        </div>
                    </div>
                    <div
                        id="morestats"
                        style={{
                            display: viewModel?.showReplaySections
                                ? "block"
                                : "none",
                        }}
                    >
                        <span id="com1">
                            {viewModel?.mainCommanderLabel ?? ""}
                        </span>
                        <span id="com2">
                            {viewModel?.allyCommanderLabel ?? ""}
                        </span>
                        <div id="map">
                            {viewModel != null ? (
                                <>
                                    {viewModel.localizedMapName}
                                    {"  "}({viewModel.lengthText}){" "}
                                    <span style={{ color: "#FFE670" }}>
                                        {viewModel.bonusText}
                                    </span>
                                </>
                            ) : null}
                        </div>
                        <div
                            id="record"
                            style={{
                                display: viewModel?.showRecord
                                    ? "block"
                                    : "none",
                            }}
                        >
                            {bestTimeText}
                        </div>
                        <span id="apm1">{viewModel?.mainAPM ?? ""}</span>
                        <span id="apm2">{viewModel?.allyAPM ?? ""}</span>
                        <div id="brutal">{viewModel?.difficultyText ?? ""}</div>
                    </div>
                </div>
                {viewModel?.commanderSections.map((section, index) => (
                    <div
                        className="commstats"
                        id={
                            index === 0
                                ? "commstats1"
                                : index === 1
                                  ? "commstats2"
                                  : "amon"
                        }
                        key={section.idPrefix}
                        style={{
                            display: viewModel.showReplaySections
                                ? "block"
                                : "none",
                        }}
                    >
                        <div
                            id={`CMname${index + 1}`}
                            style={{
                                color: section.prestigeColor ?? undefined,
                            }}
                        >
                            {section.name}
                        </div>
                        {index < 2 ? (
                            <div id={`CMicons${index + 1}`} className="icons">
                                {section.icons}
                            </div>
                        ) : null}
                        <div
                            id={index < 2 ? `CMtalent${index + 1}` : "comp"}
                            className={index < 2 ? "prestige" : undefined}
                            style={{
                                color: section.prestigeColor ?? undefined,
                            }}
                        >
                            {section.prestige}
                        </div>
                        {index < 2 ? (
                            <div
                                id={`CMmastery${index + 1}`}
                                className="mastery"
                                style={{
                                    color: section.masteryColor ?? undefined,
                                    display: section.masteryRows.some(
                                        (row) => row.value > 0,
                                    )
                                        ? "block"
                                        : "none",
                                }}
                            >
                                {renderMasteryRows(section.masteryRows)}
                            </div>
                        ) : null}
                        <div id={`CMunits${index + 1}`} className="units">
                            {renderUnitRows(
                                section.unitRows,
                                section.unitColor,
                                killsLabel,
                            )}
                        </div>
                    </div>
                ))}
            </div>
            <div id="otherstats">
                <div id="rng">{viewModel?.randomizerText ?? ""}</div>
                <div
                    id="session"
                    style={{
                        display:
                            replayModeVisible && showSessionStats
                                ? "block"
                                : "none",
                    }}
                >
                    {showSessionStats
                        ? (viewModel?.sessionText ?? fallbackSessionText)
                        : ""}
                </div>
                <div id="loader" />
            </div>
            <div
                id="mutatorinfo"
                style={{ width: showmutators ? undefined : "0" }}
            >
                {Array.from({ length: 13 }, (_, index) => (
                    <div key={`mutator-detail-${index}`}>
                        <img alt="" />
                        <p>
                            <span className="muttop" />
                            <span className="mutvalue" />
                            <br />
                            <span className="mutdesc">
                                {mutatorDescriptions[""] ?? ""}
                            </span>
                        </p>
                    </div>
                ))}
            </div>
        </>
    );
}
