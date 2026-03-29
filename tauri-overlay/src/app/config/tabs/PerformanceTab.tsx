import * as React from "react";
import type { LanguageManager } from "../../i18n/languageManager";
import type { AppSettings } from "../../../bindings/overlay";
import type { JsonValue } from "../types";
import { Grid } from "@mui/material";

type PerformanceActions = {
    isBusy: boolean;
    activeHotkeyPath: string;
    beginHotkeyCapture: (path: string) => Promise<void>;
    endHotkeyCapture: (path: string) => Promise<void>;
    hotkeyStringFromEvent: (
        event: React.KeyboardEvent<HTMLInputElement>,
    ) => string;
    triggerOverlayAction: (actionName: string) => Promise<void> | void;
    isHotkeyClearKey: (key: string) => boolean;
    isHotkeyModifierKey: (key: string) => boolean;
};

type PerformanceTabProps = {
    draft: AppSettings;
    onChange: (path: string[], value: JsonValue) => void;
    getAtPath: (source: AppSettings, path: string[]) => JsonValue | undefined;
    actions: PerformanceActions;
    displayVisibility: boolean;
    languageManager: LanguageManager;
};

export default function PerformanceTab({
    draft,
    onChange,
    getAtPath,
    actions,
    displayVisibility,
    languageManager,
}: PerformanceTabProps) {
    const t = (id: string) => languageManager.translate(id);
    const hotkeyPath = "performance_hotkey";
    const read = (
        path: string[],
        fallback: JsonValue | null = null,
    ): JsonValue | null => {
        const value = getAtPath(draft, path);
        return value === undefined ? fallback : value;
    };

    const processValues = Array.isArray(read(["performance_processes"], []))
        ? ((read(["performance_processes"], []) as string[]) || []).filter(
              (value) => typeof value === "string" && value.trim() !== "",
          )
        : [];
    const processText = processValues.join("\n");

    return (
        <Grid container>
            <Grid size={4} className="tab-content">
                <section className="card group performance-tab-card">
                    <div className="performance-tab-copy">
                        <h3>{t("ui_performance_overlay_title")}</h3>
                        <p>{t("ui_performance_overlay_description")}</p>
                        <p>{t("ui_performance_overlay_details")}</p>
                    </div>

                    <label className="performance-tab-check">
                        <input
                            type="checkbox"
                            checked={displayVisibility}
                            disabled={actions.isBusy}
                            onChange={(event) =>
                                onChange(
                                    ["performance_show"],
                                    event.target.checked,
                                )
                            }
                        />
                        <span>{t("ui_performance_show_overlay")}</span>
                    </label>

                    <button
                        type="button"
                        className="performance-tab-position-btn"
                        disabled={actions.isBusy}
                        onClick={() =>
                            actions.triggerOverlayAction(
                                "performance_toggle_reposition",
                            )
                        }
                    >
                        {t("ui_performance_change_position")}
                    </button>

                    <div className="performance-tab-hotkey-block">
                        <h3>{t("ui_performance_hotkey_title")}</h3>
                        <div className="performance-tab-hotkey-row">
                            <input
                                type="text"
                                className={`input hotkey-input ${actions.activeHotkeyPath === hotkeyPath ? "is-recording" : ""}`}
                                readOnly
                                value={String(
                                    read(["performance_hotkey"], "") || "",
                                )}
                                placeholder={
                                    actions.activeHotkeyPath === hotkeyPath
                                        ? languageManager.translate(
                                              "ui_settings_hotkey_recording",
                                          )
                                        : languageManager.translate(
                                              "ui_settings_hotkey_press_shortcut",
                                          )
                                }
                                onMouseDown={(event) => {
                                    if (
                                        actions.activeHotkeyPath === hotkeyPath
                                    ) {
                                        return;
                                    }
                                    event.preventDefault();
                                    const input = event.currentTarget;
                                    void actions.beginHotkeyCapture(hotkeyPath);
                                    window.requestAnimationFrame(() => {
                                        input.focus();
                                    });
                                }}
                                onFocus={() => {
                                    void actions.beginHotkeyCapture(hotkeyPath);
                                }}
                                onBlur={() => {
                                    void actions.endHotkeyCapture(hotkeyPath);
                                }}
                                onKeyDown={(event) => {
                                    event.preventDefault();
                                    event.stopPropagation();

                                    const input = event.currentTarget;
                                    const finishCapture = () => {
                                        void actions
                                            .endHotkeyCapture(hotkeyPath)
                                            .finally(() => {
                                                input.blur();
                                            });
                                    };

                                    if (actions.isHotkeyClearKey(event.key)) {
                                        onChange(["performance_hotkey"], "");
                                        finishCapture();
                                        return;
                                    }

                                    if (
                                        actions.isHotkeyModifierKey(event.key)
                                    ) {
                                        return;
                                    }

                                    const hotkey =
                                        actions.hotkeyStringFromEvent(event);
                                    if (hotkey !== "") {
                                        onChange(
                                            ["performance_hotkey"],
                                            hotkey,
                                        );
                                        finishCapture();
                                    }
                                }}
                            />
                        </div>
                    </div>

                    <div className="performance-tab-process-block">
                        <h3>{t("ui_performance_targets_title")}</h3>
                        <p className="performance-tab-process-copy">
                            {t("ui_performance_targets_description")}
                        </p>
                        <div className="performance-tab-process-row">
                            <textarea
                                className="input performance-tab-process-input"
                                rows={3}
                                value={processText}
                                placeholder={"SC2_x64.exe\nSC2.exe"}
                                onChange={(event) => {
                                    const nextValues = event.target.value
                                        .split("\n")
                                        .map((value) => value.trim())
                                        .filter((value) => value !== "");
                                    onChange(
                                        ["performance_processes"],
                                        nextValues,
                                    );
                                }}
                            />
                        </div>
                    </div>
                </section>
            </Grid>
        </Grid>
    );
}
