import { LanguageManager } from "../../i18n/languageManager";
import GameStatChart, { ReplayChartVisible } from "./game_stat_chart";
import GameStatText from "./game_stat_text";
import type {
    OverlayRandomizerCatalog,
    OverlayReplayPayload,
} from "../../../bindings/overlay";

type OverlayCommanderMasteryCatalog =
    OverlayRandomizerCatalog["commander_mastery"];
type OverlayPrestigeNameCatalog = OverlayRandomizerCatalog["prestige_names"];

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
    overlayCommanderMasteryCatalog: OverlayCommanderMasteryCatalog;
    overlayPrestigeNameCatalog: OverlayPrestigeNameCatalog;
    language: string;
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
                overlayLanguageManager={overlayLanguageManager}
                reportOverlayReplayDataState={reportOverlayReplayDataState}
            />
        </>
    );
}
