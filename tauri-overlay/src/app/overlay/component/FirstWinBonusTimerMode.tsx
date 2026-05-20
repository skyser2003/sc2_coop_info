import { LanguageManager } from "../../i18n/languageManager";
import type { FirstWinBonusTimerPayload } from "../../../bindings/overlay";

function formatDuration(totalSeconds: number): string {
    const boundedSeconds = Math.max(0, Math.floor(totalSeconds));
    const hours = Math.floor(boundedSeconds / 3600);
    const minutes = Math.floor((boundedSeconds % 3600) / 60);
    const seconds = boundedSeconds % 60;

    return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function formatNextAvailableTime(value?: string): string | null {
    if (value == null || value.trim() === "") {
        return null;
    }

    const date = new Date(value);
    if (!Number.isFinite(date.getTime())) {
        return null;
    }

    const year = date.getFullYear();
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    const hours24 = date.getHours();
    const period = hours24 >= 12 ? "PM" : "AM";
    const hours12 = hours24 % 12 === 0 ? 12 : hours24 % 12;
    const minutes = String(date.getMinutes()).padStart(2, "0");

    return `${year}-${month}-${day} ${hours12}:${minutes} ${period}`;
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
    const hasDetectedLatestFirstWinBonus =
        payload?.next_available_time != null &&
        payload.next_available_time.trim() !== "";
    const unmeasured = payload != null && !hasDetectedLatestFirstWinBonus;
    const nextAvailableTime = available
        ? null
        : formatNextAvailableTime(payload?.next_available_time);
    const statusText =
        payload == null
            ? ""
            : unmeasured
              ? overlayLanguageManager.translate(
                    "ui_overlay_first_win_bonus_unmeasured",
                )
              : available
                ? overlayLanguageManager.translate(
                      "ui_overlay_first_win_bonus_available",
                  )
                : overlayLanguageManager
                      .translate("ui_overlay_first_win_bonus_remaining")
                      .replace(
                          "{{time}}",
                          formatDuration(secondsUntilAvailable),
                      );

    return (
        <div
            id="firstWinBonusTimer"
            style={{
                display: "block",
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
            {nextAvailableTime != null ? (
                <div className="first-win-bonus-next-time">
                    {nextAvailableTime}
                </div>
            ) : null}
            <div className="first-win-bonus-value">{statusText}</div>
        </div>
    );
}
