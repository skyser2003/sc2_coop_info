import { LanguageManager } from "../../i18n/languageManager";
import type { FirstWinBonusTimerPayload } from "../../../bindings/overlay";

function formatDuration(totalSeconds: number): string {
    const boundedSeconds = Math.max(0, Math.floor(totalSeconds));
    const hours = Math.floor(boundedSeconds / 3600);
    const minutes = Math.floor((boundedSeconds % 3600) / 60);
    const seconds = boundedSeconds % 60;

    return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

export default function FirstWinBonusTimerMode({
    payload,
    visible,
    immediate,
    overlayLanguageManager,
}: {
    payload: FirstWinBonusTimerPayload | null;
    visible: boolean;
    immediate: boolean;
    overlayLanguageManager: LanguageManager;
}) {
    const available = payload?.available === true;
    const secondsUntilAvailable = Number(payload?.seconds_until_available ?? 0);
    const statusText = available
        ? overlayLanguageManager.translate(
              "ui_overlay_first_win_bonus_available",
          )
        : overlayLanguageManager
              .translate("ui_overlay_first_win_bonus_remaining")
              .replace("{{time}}", formatDuration(secondsUntilAvailable));

    return (
        <div
            id="firstWinBonusTimer"
            style={{
                display: visible ? "block" : "none",
                opacity: visible ? 1 : 0,
                right: visible ? "2vh" : "-38vh",
                transition: immediate ? "all 0s" : undefined,
            }}
        >
            <div className="first-win-bonus-title">
                {overlayLanguageManager.translate(
                    "ui_overlay_first_win_bonus_title",
                )}
            </div>
            <div className="first-win-bonus-value">{statusText}</div>
        </div>
    );
}
