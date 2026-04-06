import {
    type CommanderMasteryData,
    LanguageManager,
} from "../../i18n/languageManager";
import GameStatChart, { ReplayChartVisible } from "./GameStatChart";
import GameStatText from "./GameStatText";
import type { OverlayReplayPayload } from "../../../bindings/overlay";

type OverlayPrestigeNameCatalog = Record<
    string,
    { en: string[]; ko: string[] }
>;

export default function GameStatMode({
    payload,
    chartVisibility,
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
    hideNicknamesInOverlay,
    overlayLanguageManager,
    reportOverlayReplayDataState,
}: {
    payload: OverlayReplayPayload | null;
    chartVisibility: ReplayChartVisible;
    replayModeVisible: boolean;
    showSessionStats: boolean;
    sessionVictoryCount: number;
    sessionDefeatCount: number;
    p1Color: string;
    p2Color: string;
    amonColor: string | null;
    masteryColor: string | null;
    cancelReplayDisplayClearTimer: () => void;
    overlayCommanderMasteryCatalog: CommanderMasteryData;
    overlayPrestigeNameCatalog: OverlayPrestigeNameCatalog;
    language: string;
    hideNicknamesInOverlay: boolean;
    overlayLanguageManager: LanguageManager;
    reportOverlayReplayDataState: (active: boolean) => void;
}) {
    return (
        <>
            <GameStatChart
                payload={payload}
                chartVisibility={chartVisibility}
                p1Color={p1Color}
                p2Color={p2Color}
                language={language}
                languageManager={overlayLanguageManager}
            />
            <GameStatText
                payload={payload}
                replayModeVisible={replayModeVisible}
                showSessionStats={showSessionStats}
                sessionVictoryCount={sessionVictoryCount}
                sessionDefeatCount={sessionDefeatCount}
                p1Color={p1Color}
                p2Color={p2Color}
                amonColor={amonColor}
                masteryColor={masteryColor}
                cancelReplayDisplayClearTimer={cancelReplayDisplayClearTimer}
                overlayCommanderMasteryCatalog={overlayCommanderMasteryCatalog}
                overlayPrestigeNameCatalog={overlayPrestigeNameCatalog}
                language={language}
                hideNicknamesInOverlay={hideNicknamesInOverlay}
                overlayLanguageManager={overlayLanguageManager}
                reportOverlayReplayDataState={reportOverlayReplayDataState}
            />
        </>
    );
}
