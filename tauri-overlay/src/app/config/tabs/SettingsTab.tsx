import * as React from "react";
import type { LanguageManager } from "../../i18n/languageManager";
import { Grid } from "@mui/material";
import { check, Update } from "@tauri-apps/plugin-updater";
import { app } from "@tauri-apps/api";
import type { AppSettings } from "../../../bindings/overlay";
import type { DisplayValue, JsonValue } from "../types";

type SettingsActions = {
    isBusy: boolean;
    ready: boolean;
    hasPendingChanges: boolean;
    promptPath: (path: string[], title: string) => void;
    openFolderPath: (path: string) => Promise<true | null> | void;
    triggerOverlayAction: (actionName: string) => Promise<void> | void;
    activeHotkeyPath: string;
    beginHotkeyCapture: (path: string) => Promise<void>;
    endHotkeyCapture: (path: string) => Promise<void>;
    createDesktopShortcut: () => Promise<void> | void;
    parseReplayPrompt: () => Promise<void> | void;
    overlayScreenshot: () => Promise<void> | void;
    runDetailedAnalysis: () => Promise<void> | void;
    startSimpleAnalysis: () => Promise<void> | void;
    pauseDetailedAnalysis: () => Promise<void> | void;
    deleteParsedData: () => Promise<void> | void;
    applyMainSettings: () => Promise<void> | void;
    resetMainSettings: () => void;
    isHotkeyClearKey: (key: string) => boolean;
    isHotkeyModifierKey: (key: string) => boolean;
    detailedAnalysisRunning?: boolean;
    simpleAnalysisRunning?: boolean;
    detailedAnalysisStatus?: string;
    simpleAnalysisStatus?: string;
    analysisMessage?: string;
    analysisScanProgress?: Record<string, JsonValue> | null;
    analysisTotalValidFiles?: number;
    analysisDetailedParsedCount?: number;
    monitorOptions?: Array<{
        index: number;
        label: string;
    }>;
};

const HEX_COLOR_PATTERN = /^#[0-9A-F]{6}$/i;
type SettingsTabProps = {
    draft: AppSettings | null;
    onChange: (path: string[], value: JsonValue) => void;
    getAtPath?: (
        source: AppSettings | null,
        path: string[],
    ) => JsonValue | undefined;
    asTableValue?: (value: DisplayValue) => string;
    hotkeyStringFromEvent?: (
        event: React.KeyboardEvent<HTMLInputElement>,
    ) => string;
    actions: SettingsActions;
    languageManager: LanguageManager;
};

function asTableValueCompat(value: DisplayValue) {
    if (value === null || value === undefined) {
        return "";
    }
    return String(value);
}

function getAtPathCompat(source: AppSettings | null, path: string[]) {
    return path.reduce(
        (acc: JsonValue | undefined, key) =>
            acc != null && typeof acc === "object"
                ? (acc as Record<string, JsonValue>)[key]
                : undefined,
        source as JsonValue | undefined,
    );
}

function hotkeyStringFromEventCompat(
    event: React.KeyboardEvent<HTMLInputElement>,
) {
    const baseKey = event.key;
    if (!baseKey) {
        return "";
    }
    if (baseKey === "Backspace" || baseKey === "Delete") {
        return "";
    }

    const modifiers = [];
    if (event.ctrlKey) modifiers.push("Ctrl");
    if (event.altKey) modifiers.push("Alt");
    if (event.shiftKey) modifiers.push("Shift");
    if (event.metaKey) modifiers.push("Meta");

    const ignored = new Set(["Control", "Shift", "Alt", "Meta"]);
    if (ignored.has(baseKey)) {
        return modifiers.join("+");
    }

    const keyMap = {
        " ": "Space",
        ArrowUp: "Up",
        ArrowDown: "Down",
        ArrowLeft: "Left",
        ArrowRight: "Right",
        PageUp: "PageUp",
        PageDown: "PageDown",
        Home: "Home",
        End: "End",
        Insert: "Insert",
        Enter: "Enter",
        Tab: "Tab",
    };

    const codeMap = {
        Digit0: "0",
        Digit1: "1",
        Digit2: "2",
        Digit3: "3",
        Digit4: "4",
        Digit5: "5",
        Digit6: "6",
        Digit7: "7",
        Digit8: "8",
        Digit9: "9",
        Minus: "-",
        Equal: "=",
        BracketLeft: "[",
        BracketRight: "]",
        Backslash: "\\",
        Semicolon: ";",
        Quote: "'",
        Comma: ",",
        Period: ".",
        Slash: "/",
        Backquote: "`",
        NumpadMultiply: "*",
        NumpadDivide: "/",
        NumpadSubtract: "-",
        NumpadAdd: "+",
        NumpadDecimal: ".",
        NumpadEnter: "Enter",
    };

    let key = codeMap[event.code] || keyMap[baseKey] || baseKey;
    if (key.length === 1 && /[a-z]/i.test(key)) {
        key = key.toUpperCase();
    }
    return [...modifiers, key].join("+");
}

function translateText(
    languageManager: LanguageManager,
    id: string,
    values: Record<string, string | number> = {},
) {
    return Object.entries(values).reduce(
        (text, [key, value]) => text.split(`{{${key}}}`).join(String(value)),
        languageManager.translate(id),
    );
}

function formatNumber(value: DisplayValue) {
    const num = Number(value);
    if (!Number.isFinite(num)) {
        return asTableValueCompat(value);
    }
    return num.toLocaleString("en-US");
}

function formatDurationSeconds(totalSeconds: number) {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function getLogicalCoreCount() {
    if (typeof navigator === "undefined") {
        return 1;
    }

    return Math.max(1, Math.trunc(navigator.hardwareConcurrency || 1));
}

function getDefaultAnalysisWorkerThreads() {
    return Math.max(1, Math.floor(getLogicalCoreCount() / 2));
}

function renderAnalysisProgress(
    progressInput: Record<string, JsonValue> | null | undefined,
    languageManager: LanguageManager,
    totalValidFiles?: number,
    detailedParsedCount?: number,
    preferProgressTotal?: boolean,
) {
    const progress = progressInput || {};
    const total = Number(progress.total_replay_files ?? progress.total ?? 0);
    const failed = Number(progress.parse_failed_files ?? progress.failed ?? 0);
    const completed = Number(
        progress.completed ??
            Number(progress.files_already_cached ?? progress.cache_hits ?? 0) +
                Number(
                    progress.newly_parsed ?? progress.newly_parsed_files ?? 0,
                ),
    );
    const safeTotal =
        preferProgressTotal && total > 0
            ? total
            : Number(totalValidFiles ?? 0) > 0
              ? Number(totalValidFiles)
              : Math.max(total - failed, 0);
    const settledDetailedCount = Math.max(Number(detailedParsedCount ?? 0), 0);
    const safeCompleted = Math.min(
        preferProgressTotal ? Math.max(completed, 0) : settledDetailedCount,
        safeTotal ||
            (preferProgressTotal
                ? Math.max(completed, 0)
                : settledDetailedCount),
    );
    const progressPercent =
        safeTotal > 0 ? Math.min((safeCompleted / safeTotal) * 100, 100) : 0;

    return (
        <>
            <div className="analysis-progress-group">
                <div
                    className="analysis-progress-bar"
                    role="progressbar"
                    aria-valuemin={0}
                    aria-valuemax={safeTotal}
                    aria-valuenow={safeCompleted}
                    aria-label={languageManager.translate("ui_stats_progress")}
                >
                    <div
                        className="analysis-progress-fill"
                        style={{ width: `${progressPercent}%` }}
                    />
                </div>
            </div>
            <p className="note analysis-progress-count">
                {translateText(languageManager, "ui_stats_progress", {
                    current: formatNumber(safeCompleted),
                    total: formatNumber(safeTotal),
                })}
            </p>
            <p className="note">
                {translateText(languageManager, "ui_stats_failed_files", {
                    value: formatNumber(failed),
                })}
            </p>
        </>
    );
}

function normalizeHexColor(value: DisplayValue, fallback: string = "#FFFFFF") {
    if (typeof value !== "string") {
        return fallback;
    }
    const normalized = value.trim();
    return HEX_COLOR_PATTERN.test(normalized)
        ? normalized.toUpperCase()
        : fallback;
}

type ColorFieldProps = {
    label: string;
    path: string[];
    color: string;
    disabled: boolean;
    onChange: (path: string[], value: JsonValue) => void;
};

type RgbColor = {
    r: number;
    g: number;
    b: number;
};

type HsvColor = {
    h: number;
    s: number;
    v: number;
};

function clamp(value: number, min: number, max: number) {
    return Math.min(Math.max(value, min), max);
}

function hexToRgb(value: string): RgbColor {
    const normalized = normalizeHexColor(value, "#FFFFFF").slice(1);
    return {
        r: parseInt(normalized.slice(0, 2), 16),
        g: parseInt(normalized.slice(2, 4), 16),
        b: parseInt(normalized.slice(4, 6), 16),
    };
}

function rgbToHex({ r, g, b }: RgbColor): string {
    const toHex = (channel: number) =>
        clamp(Math.round(channel), 0, 255)
            .toString(16)
            .padStart(2, "0")
            .toUpperCase();
    return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}

function rgbToHsv({ r, g, b }: RgbColor): HsvColor {
    const red = r / 255;
    const green = g / 255;
    const blue = b / 255;
    const max = Math.max(red, green, blue);
    const min = Math.min(red, green, blue);
    const delta = max - min;

    let hue = 0;
    if (delta !== 0) {
        if (max === red) {
            hue = 60 * (((green - blue) / delta) % 6);
        } else if (max === green) {
            hue = 60 * ((blue - red) / delta + 2);
        } else {
            hue = 60 * ((red - green) / delta + 4);
        }
    }

    return {
        h: (hue + 360) % 360,
        s: max === 0 ? 0 : delta / max,
        v: max,
    };
}

function hsvToRgb({ h, s, v }: HsvColor): RgbColor {
    const hue = ((h % 360) + 360) % 360;
    const saturation = clamp(s, 0, 1);
    const value = clamp(v, 0, 1);
    const chroma = value * saturation;
    const segment = hue / 60;
    const x = chroma * (1 - Math.abs((segment % 2) - 1));
    const match = value - chroma;

    let red = 0;
    let green = 0;
    let blue = 0;

    if (segment >= 0 && segment < 1) {
        red = chroma;
        green = x;
    } else if (segment >= 1 && segment < 2) {
        red = x;
        green = chroma;
    } else if (segment >= 2 && segment < 3) {
        green = chroma;
        blue = x;
    } else if (segment >= 3 && segment < 4) {
        green = x;
        blue = chroma;
    } else if (segment >= 4 && segment < 5) {
        red = x;
        blue = chroma;
    } else {
        red = chroma;
        blue = x;
    }

    return {
        r: (red + match) * 255,
        g: (green + match) * 255,
        b: (blue + match) * 255,
    };
}

function hexToHsv(value: string): HsvColor {
    return rgbToHsv(hexToRgb(value));
}

function hsvToHex(value: HsvColor): string {
    return rgbToHex(hsvToRgb(value));
}

function hueColorHex(hue: number): string {
    return hsvToHex({ h: hue, s: 1, v: 1 });
}

const ColorField = React.memo(function ColorField({
    label,
    path,
    color,
    disabled,
    onChange,
}: ColorFieldProps) {
    const [hsv, setHsv] = React.useState<HsvColor>(() => hexToHsv(color));
    const [textValue, setTextValue] = React.useState(color);
    const [isOpen, setIsOpen] = React.useState(false);
    const rootRef = React.useRef<HTMLDivElement | null>(null);
    const wheelRef = React.useRef<HTMLDivElement | null>(null);
    const squareRef = React.useRef<HTMLDivElement | null>(null);
    const lastEmittedColorRef = React.useRef(color);

    React.useEffect(() => {
        lastEmittedColorRef.current = color;
        setHsv(hexToHsv(color));
        setTextValue(color);
    }, [color]);

    React.useEffect(() => {
        if (!isOpen) {
            return undefined;
        }

        function handlePointerDown(event: PointerEvent) {
            const rootElement = rootRef.current;
            if (rootElement === null) {
                return;
            }
            const target = event.target;
            if (target instanceof Node && !rootElement.contains(target)) {
                setIsOpen(false);
            }
        }

        function handleKeyDown(event: KeyboardEvent) {
            if (event.key === "Escape") {
                setIsOpen(false);
            }
        }

        window.addEventListener("pointerdown", handlePointerDown);
        window.addEventListener("keydown", handleKeyDown);
        return () => {
            window.removeEventListener("pointerdown", handlePointerDown);
            window.removeEventListener("keydown", handleKeyDown);
        };
    }, [isOpen]);

    function emitColor(nextHsv: HsvColor) {
        const nextColor = hsvToHex(nextHsv);
        setHsv(nextHsv);
        setTextValue(nextColor);
        if (nextColor === lastEmittedColorRef.current) {
            return;
        }
        lastEmittedColorRef.current = nextColor;
        onChange(path, nextColor);
    }

    function commitTextColor(value: string) {
        const normalized = value.trim().toUpperCase();
        if (!HEX_COLOR_PATTERN.test(normalized)) {
            setTextValue(lastEmittedColorRef.current);
            return;
        }

        setTextValue(normalized);
        if (normalized === lastEmittedColorRef.current) {
            setHsv(hexToHsv(normalized));
            return;
        }

        lastEmittedColorRef.current = normalized;
        setHsv(hexToHsv(normalized));
        onChange(path, normalized);
    }

    function updateHueFromPointer(event: React.PointerEvent<HTMLDivElement>) {
        const wheelElement = wheelRef.current;
        if (wheelElement === null) {
            return;
        }
        const rect = wheelElement.getBoundingClientRect();
        const centerX = rect.left + rect.width / 2;
        const centerY = rect.top + rect.height / 2;
        const angleRadians = Math.atan2(
            event.clientY - centerY,
            event.clientX - centerX,
        );
        const hue = ((angleRadians * 180) / Math.PI + 90 + 360) % 360;
        emitColor({ ...hsv, h: hue });
    }

    function updateSquareFromPointer(
        event: React.PointerEvent<HTMLDivElement>,
    ) {
        const squareElement = squareRef.current;
        if (squareElement === null) {
            return;
        }
        const rect = squareElement.getBoundingClientRect();
        const saturation = clamp(
            (event.clientX - rect.left) / rect.width,
            0,
            1,
        );
        const value = clamp(1 - (event.clientY - rect.top) / rect.height, 0, 1);
        emitColor({ ...hsv, s: saturation, v: value });
    }

    const localColor = hsvToHex(hsv);
    const hueColor = hueColorHex(hsv.h);
    const wheelRadians = ((hsv.h - 90) * Math.PI) / 180;
    const wheelMarkerStyle = {
        left: `${50 + Math.cos(wheelRadians) * 44}%`,
        top: `${50 + Math.sin(wheelRadians) * 44}%`,
    };
    const squareMarkerStyle = {
        left: `${hsv.s * 100}%`,
        top: `${(1 - hsv.v) * 100}%`,
    };

    return (
        <div key={path.join(".")} ref={rootRef} className="color-row">
            <span className="color-row-label">{label}</span>
            <div className="color-row-input">
                <button
                    type="button"
                    className="color-row-input-button"
                    onClick={() => {
                        if (disabled) {
                            return;
                        }
                        setIsOpen((current) => !current);
                    }}
                    aria-expanded={isOpen}
                    disabled={disabled}
                >
                    <span
                        className="color-row-swatch"
                        aria-hidden="true"
                        style={{ backgroundColor: localColor }}
                    />
                </button>
            </div>
            <div className="color-row-popup-anchor">
                {isOpen ? (
                    <div
                        className="color-picker-popup"
                        data-disabled={String(disabled)}
                    >
                        <input
                            type="text"
                            className="input color-row-text"
                            value={textValue}
                            inputMode="text"
                            spellCheck={false}
                            maxLength={7}
                            disabled={disabled}
                            aria-label={`${label} color value`}
                            onChange={(event) => {
                                setTextValue(event.target.value.toUpperCase());
                            }}
                            onBlur={(event) => {
                                commitTextColor(event.target.value);
                            }}
                            onKeyDown={(event) => {
                                if (event.key === "Enter") {
                                    commitTextColor(event.currentTarget.value);
                                    event.currentTarget.blur();
                                } else if (event.key === "Escape") {
                                    setTextValue(lastEmittedColorRef.current);
                                    event.currentTarget.blur();
                                }
                            }}
                        />
                        <div className="color-wheel-picker">
                            <div
                                ref={wheelRef}
                                className="color-wheel-ring"
                                onPointerDown={(event) => {
                                    if (disabled) {
                                        return;
                                    }
                                    event.currentTarget.setPointerCapture(
                                        event.pointerId,
                                    );
                                    updateHueFromPointer(event);
                                }}
                                onPointerMove={(event) => {
                                    if (
                                        !event.currentTarget.hasPointerCapture(
                                            event.pointerId,
                                        )
                                    ) {
                                        return;
                                    }
                                    updateHueFromPointer(event);
                                }}
                                onPointerUp={(event) => {
                                    if (
                                        event.currentTarget.hasPointerCapture(
                                            event.pointerId,
                                        )
                                    ) {
                                        event.currentTarget.releasePointerCapture(
                                            event.pointerId,
                                        );
                                    }
                                }}
                            >
                                <div
                                    className="color-wheel-ring-marker"
                                    style={wheelMarkerStyle}
                                />
                            </div>
                            <div
                                ref={squareRef}
                                className="color-wheel-square"
                                style={{
                                    backgroundImage: `linear-gradient(to right, #FFFFFF, ${hueColor}), linear-gradient(to top, #000000, transparent)`,
                                }}
                                onPointerDown={(event) => {
                                    if (disabled) {
                                        return;
                                    }
                                    event.stopPropagation();
                                    event.currentTarget.setPointerCapture(
                                        event.pointerId,
                                    );
                                    updateSquareFromPointer(event);
                                }}
                                onPointerMove={(event) => {
                                    if (
                                        !event.currentTarget.hasPointerCapture(
                                            event.pointerId,
                                        )
                                    ) {
                                        return;
                                    }
                                    updateSquareFromPointer(event);
                                }}
                                onPointerUp={(event) => {
                                    if (
                                        event.currentTarget.hasPointerCapture(
                                            event.pointerId,
                                        )
                                    ) {
                                        event.currentTarget.releasePointerCapture(
                                            event.pointerId,
                                        );
                                    }
                                }}
                            >
                                <div
                                    className="color-wheel-square-marker"
                                    style={squareMarkerStyle}
                                />
                            </div>
                        </div>
                    </div>
                ) : null}
            </div>
        </div>
    );
});

export default function SettingsTab({
    draft,
    onChange,
    getAtPath = getAtPathCompat,
    asTableValue = asTableValueCompat,
    hotkeyStringFromEvent = hotkeyStringFromEventCompat,
    actions,
    languageManager,
}: SettingsTabProps) {
    const t = (id: string) => languageManager.translate(id);
    const read = (
        path: string[],
        fallback: JsonValue | null = null,
    ): JsonValue | null => {
        const value = getAtPath(draft, path);
        return value === undefined ? fallback : value;
    };

    const boolField = (
        label: string,
        path: string[],
        fallback = false,
        disabled = false,
    ) => (
        <label
            className={`main-setting-check ${disabled ? "is-disabled" : ""}`}
            key={path.join(".")}
        >
            <input
                type="checkbox"
                checked={Boolean(read(path, fallback))}
                disabled={disabled}
                onChange={(event) => onChange(path, event.target.checked)}
            />
            <span>{label}</span>
        </label>
    );

    const minimizeToTrayEnabled = Boolean(read(["minimize_to_tray"], false));
    const detailedAnalysisRunning = Boolean(actions.detailedAnalysisRunning);
    const simpleAnalysisRunning = Boolean(actions.simpleAnalysisRunning);
    const detailedAnalysisStatus =
        actions.detailedAnalysisStatus || t("ui_stats_detailed_not_started");
    const simpleAnalysisStatus =
        actions.simpleAnalysisStatus || t("ui_stats_waiting_simple_startup");
    const analysisMessage =
        asTableValue(actions.analysisMessage) || t("ui_stats_no_statistics");
    const analysisScanProgress = actions.analysisScanProgress || null;
    const analysisTotalValidFiles = Number(
        actions.analysisTotalValidFiles ?? 0,
    );
    const analysisDetailedParsedCount = Number(
        actions.analysisDetailedParsedCount ?? 0,
    );
    const normalizedAnalysisMessage = analysisMessage.trim();
    const normalizedSimpleAnalysisStatus =
        asTableValue(simpleAnalysisStatus).trim();
    const normalizedDetailedAnalysisStatus = asTableValue(
        detailedAnalysisStatus,
    ).trim();
    const showAnalysisMessage =
        normalizedAnalysisMessage.length > 0 &&
        normalizedAnalysisMessage !== normalizedSimpleAnalysisStatus &&
        normalizedAnalysisMessage !== normalizedDetailedAnalysisStatus;
    const showSimpleAnalysisStatus =
        normalizedSimpleAnalysisStatus.length > 0 &&
        (simpleAnalysisRunning ||
            (!detailedAnalysisRunning &&
                normalizedSimpleAnalysisStatus !==
                    t("ui_stats_waiting_simple_startup")));
    const showDetailedAnalysisStatus =
        normalizedDetailedAnalysisStatus.length > 0;
    const generalAnalysisStatus =
        (detailedAnalysisRunning
            ? normalizedDetailedAnalysisStatus
            : showAnalysisMessage
              ? normalizedAnalysisMessage
              : showSimpleAnalysisStatus
                ? normalizedSimpleAnalysisStatus
                : normalizedDetailedAnalysisStatus
        ).trim() || t("ui_stats_analysis_idle");

    const monitorOptions =
        actions.monitorOptions && actions.monitorOptions.length > 0
            ? actions.monitorOptions
            : [
                  {
                      index: Number(read(["monitor"], 1) || 1),
                      label: `${t("ui_settings_monitor")} ${Number(read(["monitor"], 1) || 1)}`,
                  },
              ];
    const logicalCoreCount = getLogicalCoreCount();
    const defaultAnalysisWorkerThreads = getDefaultAnalysisWorkerThreads();
    const analysisWorkerThreads = clamp(
        Number(
            read(["analysis_worker_threads"], defaultAnalysisWorkerThreads) ||
                defaultAnalysisWorkerThreads,
        ),
        1,
        logicalCoreCount,
    );
    const updateAnalysisWorkerThreads = (value: number) => {
        onChange(
            ["analysis_worker_threads"],
            clamp(
                Math.round(value || defaultAnalysisWorkerThreads),
                1,
                logicalCoreCount,
            ),
        );
    };

    const hotkeyEntry = (
        id: string,
        label: string,
        path: string[],
        actionName: string,
    ) => {
        const hotkeyPath = path.join(".");

        return (
            <Grid
                container
                columns={10}
                spacing={1.25}
                alignItems="stretch"
                className="hotkey-entry"
                key={id}
            >
                <Grid size={4}>
                    <button
                        type="button"
                        className="hotkey-action-btn button-normal"
                        onClick={() => actions.triggerOverlayAction(actionName)}
                        disabled={actions.isBusy}
                    >
                        {label}
                    </button>
                </Grid>
                <Grid size={6}>
                    <input
                        type="text"
                        className={`input hotkey-input ${actions.activeHotkeyPath === hotkeyPath ? "is-recording" : ""}`}
                        readOnly
                        value={String(read(path, "") || "")}
                        placeholder={
                            actions.activeHotkeyPath === hotkeyPath
                                ? t("ui_settings_hotkey_recording")
                                : t("ui_settings_hotkey_press_shortcut")
                        }
                        onMouseDown={(event) => {
                            if (actions.activeHotkeyPath === hotkeyPath) {
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
                                onChange(path, "");
                                finishCapture();
                                return;
                            }

                            if (actions.isHotkeyModifierKey(event.key)) {
                                return;
                            }

                            const hotkey = hotkeyStringFromEvent(event);
                            if (hotkey !== "") {
                                onChange(path, hotkey);
                                finishCapture();
                            }
                        }}
                    />
                </Grid>
            </Grid>
        );
    };

    const colorField = (label: string, path: string[]) => {
        const color = normalizeHexColor(read(path, "#FFFFFF"));
        return (
            <ColorField
                key={path.join(".")}
                label={label}
                path={path}
                color={color}
                disabled={actions.isBusy}
                onChange={onChange}
            />
        );
    };

    const checkUpdate = (event: React.MouseEvent<HTMLButtonElement>) => {
        (async () => {
            const update = await check();

            if (update) {
                const version = update.version;
                const confirmText = `${t("ui_update_confirm_question")} (v${version})`;
                const confirmed = confirm(confirmText);

                if (confirmed) {
                    await performUpdate(update);
                }
            } else {
                let appVersion = "vUnknown";

                app.getVersion()
                    .then((version) => {
                        appVersion = version;
                    })
                    .finally(() => {
                        alert(
                            `${t("ui_update_no_update_exists")} (v${appVersion})`,
                        );
                    });
            }
        })();
    };

    const performUpdate = async (update: Update) => {
        let downloaded = 0;
        let contentLength = 0;

        await update.downloadAndInstall((event) => {
            switch (event.event) {
                case "Started":
                    contentLength = event.data.contentLength;
                    console.log(
                        `started downloading ${event.data.contentLength} bytes`,
                    );
                    break;
                case "Progress":
                    downloaded += event.data.chunkLength;
                    console.log(
                        `downloaded ${downloaded} from ${contentLength}`,
                    );
                    break;
                case "Finished":
                    console.log("download finished");
                    break;
            }
        });

        console.log("update installed");
    };

    return (
        <div className="tab-content main-settings-content">
            <Grid container className="card">
                <Grid size={4}>
                    <div className="main-settings-top">
                        <div className="main-settings-groups">
                            <section className="main-settings-group">
                                <h3 className="main-settings-group-title">
                                    {t("ui_settings_launch_setting")}
                                </h3>
                                <div className="main-settings-group-fields">
                                    {boolField(
                                        t("ui_settings_start_with_windows"),
                                        ["start_with_windows"],
                                    )}
                                    {boolField(
                                        t("ui_settings_minimize_to_tray"),
                                        ["minimize_to_tray"],
                                    )}
                                    {boolField(
                                        t("ui_settings_start_minimized"),
                                        ["start_minimized"],
                                        false,
                                        !minimizeToTrayEnabled,
                                    )}{" "}
                                    {boolField(
                                        t("ui_settings_auto_update"),
                                        ["auto_update"],
                                        true,
                                    )}
                                </div>
                            </section>
                            <section className="main-settings-group">
                                <h3 className="main-settings-group-title">
                                    {t("ui_settings_overlay_options")}
                                </h3>
                                <div className="main-settings-group-fields">
                                    <Grid
                                        container
                                        spacing={1}
                                        className="main-number-row"
                                    >
                                        <Grid>
                                            <span className="main-row-label">
                                                {t("ui_settings_duration")}
                                            </span>
                                        </Grid>
                                        <Grid>
                                            <input
                                                className="input"
                                                type="number"
                                                min={1}
                                                max={9999}
                                                value={Number(
                                                    read(["duration"], 60) ||
                                                        60,
                                                )}
                                                onChange={(event) =>
                                                    onChange(
                                                        ["duration"],
                                                        Math.max(
                                                            1,
                                                            Number(
                                                                event.target
                                                                    .value,
                                                            ) || 60,
                                                        ),
                                                    )
                                                }
                                            />
                                        </Grid>
                                    </Grid>
                                    {boolField(
                                        t(
                                            "ui_settings_show_player_winrates_and_notes",
                                        ),
                                        ["show_player_winrates"],
                                    )}
                                    {boolField(
                                        t(
                                            "ui_settings_show_replay_info_after_game",
                                        ),
                                        ["show_replay_info_after_game"],
                                    )}
                                    {boolField(
                                        t("ui_settings_show_session_stats"),
                                        ["show_session"],
                                    )}
                                    {boolField(
                                        t("ui_settings_show_charts"),
                                        ["show_charts"],
                                        true,
                                    )}
                                    {boolField(
                                        t(
                                            "ui_settings_hide_nicknames_in_overlay",
                                        ),
                                        ["hide_nicknames_in_overlay"],
                                    )}
                                </div>
                            </section>
                            <section className="main-settings-group">
                                <h3 className="main-settings-group-title">
                                    {t(
                                        "ui_statistics_subtab_detailed_analysis",
                                    )}
                                </h3>
                                <div className="main-settings-group-fields">
                                    <p className="note">
                                        {t("ui_stats_detailed_description")}
                                    </p>
                                    <p className="note">
                                        {t("ui_stats_detailed_warning")}
                                    </p>
                                    <Grid container className="main-range-row">
                                        <Grid
                                            size={4}
                                            className="main-range-header"
                                        >
                                            <span className="main-row-label">
                                                {t(
                                                    "ui_settings_analysis_worker_threads",
                                                )}
                                            </span>
                                        </Grid>
                                        <Grid
                                            size={8}
                                            className="main-range-controls"
                                        >
                                            <input
                                                type="range"
                                                className="main-range-input"
                                                min={1}
                                                max={logicalCoreCount}
                                                step={1}
                                                value={analysisWorkerThreads}
                                                aria-label={t(
                                                    "ui_settings_analysis_worker_threads",
                                                )}
                                                onChange={(event) =>
                                                    updateAnalysisWorkerThreads(
                                                        Number(
                                                            event.target.value,
                                                        ),
                                                    )
                                                }
                                            />
                                            <input
                                                type="number"
                                                className="input main-range-number"
                                                min={1}
                                                max={logicalCoreCount}
                                                step={1}
                                                value={analysisWorkerThreads}
                                                aria-label={t(
                                                    "ui_settings_analysis_worker_threads",
                                                )}
                                                onChange={(event) =>
                                                    updateAnalysisWorkerThreads(
                                                        Number(
                                                            event.target.value,
                                                        ),
                                                    )
                                                }
                                            />
                                        </Grid>
                                    </Grid>
                                    <div className="toolbar">
                                        <button
                                            type="button"
                                            className="button-normal"
                                            onClick={
                                                actions.runDetailedAnalysis
                                            }
                                            disabled={
                                                actions.isBusy ||
                                                detailedAnalysisRunning ||
                                                simpleAnalysisRunning
                                            }
                                        >
                                            {detailedAnalysisRunning
                                                ? t("ui_stats_detailed_running")
                                                : t(
                                                      "ui_stats_run_detailed_analysis",
                                                  )}
                                        </button>
                                        <button
                                            type="button"
                                            className="button-normal"
                                            onClick={
                                                actions.startSimpleAnalysis
                                            }
                                            disabled={
                                                actions.isBusy ||
                                                actions.ready ||
                                                detailedAnalysisRunning ||
                                                simpleAnalysisRunning
                                            }
                                        >
                                            {simpleAnalysisRunning
                                                ? t("ui_stats_simple_running")
                                                : t(
                                                      "ui_stats_run_simple_analysis",
                                                  )}
                                        </button>
                                        <button
                                            type="button"
                                            className="button-normal"
                                            onClick={
                                                actions.pauseDetailedAnalysis
                                            }
                                            disabled={
                                                actions.isBusy ||
                                                !detailedAnalysisRunning
                                            }
                                        >
                                            {t("ui_stats_pause")}
                                        </button>
                                        <button
                                            type="button"
                                            className="button-normal"
                                            onClick={actions.deleteParsedData}
                                            disabled={
                                                actions.isBusy ||
                                                detailedAnalysisRunning
                                            }
                                        >
                                            {t("ui_stats_delete_parsed_data")}
                                        </button>
                                    </div>
                                    {boolField(
                                        t(
                                            "ui_stats_detailed_analysis_at_start",
                                        ),
                                        ["detailed_analysis_atstart"],
                                    )}
                                    <p className="note">
                                        {generalAnalysisStatus}
                                    </p>
                                    {renderAnalysisProgress(
                                        analysisScanProgress,
                                        languageManager,
                                        analysisTotalValidFiles,
                                        analysisDetailedParsedCount,
                                        detailedAnalysisRunning,
                                    )}
                                </div>
                            </section>
                            <section className="main-settings-group">
                                <h3 className="main-settings-group-title">
                                    {t("ui_settings_etc")}
                                </h3>
                                <Grid
                                    container
                                    className="main-settings-group-fields main-settings-inline-numbers"
                                    spacing={1.25}
                                >
                                    <Grid size={12}>
                                        <Grid
                                            container
                                            columns={10}
                                            spacing={1.25}
                                            alignItems="center"
                                            className="main-settings-row-grid"
                                        >
                                            <Grid size={4}>
                                                <span className="main-row-label">
                                                    {t(
                                                        "ui_settings_language_label",
                                                    )}
                                                </span>
                                            </Grid>
                                            <Grid size={6}>
                                                <select
                                                    className="input main-fixed-select"
                                                    value={String(
                                                        read(
                                                            ["language"],
                                                            "en",
                                                        ) || "en",
                                                    )}
                                                    onChange={(event) =>
                                                        onChange(
                                                            ["language"],
                                                            event.target.value,
                                                        )
                                                    }
                                                >
                                                    <option value="en">
                                                        {t(
                                                            "ui_language_english",
                                                        )}
                                                    </option>
                                                    <option value="ko">
                                                        {t(
                                                            "ui_language_korean",
                                                        )}
                                                    </option>
                                                </select>
                                            </Grid>
                                        </Grid>
                                    </Grid>
                                    <Grid size={12}>
                                        <Grid
                                            container
                                            columns={10}
                                            spacing={1.25}
                                            alignItems="center"
                                            className="main-settings-row-grid"
                                        >
                                            <Grid size={4}>
                                                <span className="main-row-label">
                                                    {t("ui_settings_monitor")}
                                                </span>
                                            </Grid>
                                            <Grid size={6}>
                                                <select
                                                    className="input main-fixed-select"
                                                    value={Number(
                                                        read(["monitor"], 1) ||
                                                            1,
                                                    )}
                                                    onChange={(event) =>
                                                        onChange(
                                                            ["monitor"],
                                                            Math.max(
                                                                1,
                                                                Number(
                                                                    event.target
                                                                        .value,
                                                                ) || 1,
                                                            ),
                                                        )
                                                    }
                                                >
                                                    {monitorOptions.map(
                                                        (option) => (
                                                            <option
                                                                key={
                                                                    option.index
                                                                }
                                                                value={
                                                                    option.index
                                                                }
                                                            >
                                                                {option.label}
                                                            </option>
                                                        ),
                                                    )}
                                                </select>
                                            </Grid>
                                        </Grid>
                                    </Grid>
                                    <Grid size={12}>
                                        {boolField(
                                            t("ui_settings_enable_logging"),
                                            ["enable_logging"],
                                        )}
                                    </Grid>
                                    <Grid size={12}>
                                        {boolField(
                                            t("ui_settings_dark_theme"),
                                            ["dark_theme"],
                                        )}
                                    </Grid>
                                </Grid>
                            </section>
                        </div>
                    </div>
                </Grid>
                <Grid size={4}>
                    <div className="main-settings-top">
                        <div className="main-settings-groups">
                            <div className="main-settings-group">
                                <h3 className="main-settings-group-title">
                                    {t("ui_settings_paths_description")}
                                </h3>
                                <Grid container>
                                    <Grid size={8}>
                                        <p className="main-path-value mono">
                                            {asTableValue(
                                                read(
                                                    ["account_folder"],
                                                    t(
                                                        "ui_settings_account_folder_empty",
                                                    ),
                                                ),
                                            )}
                                        </p>
                                    </Grid>
                                    <Grid>
                                        <button
                                            type="button"
                                            className="main-path-btn button-normal"
                                            onClick={() =>
                                                actions.promptPath(
                                                    ["account_folder"],
                                                    t(
                                                        "ui_settings_account_folder_path_title",
                                                    ),
                                                )
                                            }
                                            disabled={actions.isBusy}
                                        >
                                            {t("ui_settings_account_folder")}
                                        </button>
                                    </Grid>
                                    <Grid>
                                        <button
                                            type="button"
                                            className="main-path-btn button-normal"
                                            style={{ marginLeft: "5px" }}
                                            onClick={() =>
                                                actions.openFolderPath(
                                                    asTableValue(
                                                        read(
                                                            ["account_folder"],
                                                            "",
                                                        ),
                                                    ),
                                                )
                                            }
                                            disabled={actions.isBusy}
                                        >
                                            {t(
                                                "ui_settings_open_account_folder",
                                            )}
                                        </button>
                                    </Grid>
                                </Grid>
                                <Grid container>
                                    <Grid size={8}>
                                        <p className="main-path-value mono">
                                            {asTableValue(
                                                read(
                                                    ["screenshot_folder"],
                                                    t(
                                                        "ui_settings_screenshot_folder_empty",
                                                    ),
                                                ),
                                            )}
                                        </p>
                                    </Grid>
                                    <Grid>
                                        <button
                                            type="button"
                                            className="main-path-btn button-normal"
                                            onClick={() =>
                                                actions.promptPath(
                                                    ["screenshot_folder"],
                                                    t(
                                                        "ui_settings_screenshot_folder_path_title",
                                                    ),
                                                )
                                            }
                                            disabled={actions.isBusy}
                                        >
                                            {t("ui_settings_screenshot_folder")}
                                        </button>
                                    </Grid>
                                    <Grid>
                                        <button
                                            type="button"
                                            className="main-path-btn button-normal"
                                            style={{ marginLeft: "5px" }}
                                            onClick={() =>
                                                actions.openFolderPath(
                                                    asTableValue(
                                                        read(
                                                            [
                                                                "screenshot_folder",
                                                            ],
                                                            "",
                                                        ),
                                                    ),
                                                )
                                            }
                                            disabled={actions.isBusy}
                                        >
                                            {t(
                                                "ui_settings_open_screenshot_folder",
                                            )}
                                        </button>
                                    </Grid>
                                </Grid>
                            </div>
                            <div className="main-settings-group">
                                <h3 className="main-settings-group-title">
                                    {t("ui_settings_hotkeys")}
                                </h3>
                                <Grid
                                    container
                                    spacing={1.25}
                                    className="hotkeys-grid"
                                >
                                    {hotkeyEntry(
                                        "showhide",
                                        t("ui_settings_hotkey_show_hide"),
                                        ["hotkey_show/hide"],
                                        "overlay_show_hide",
                                    )}
                                    {hotkeyEntry(
                                        "show",
                                        t("ui_settings_hotkey_show"),
                                        ["hotkey_show"],
                                        "overlay_show",
                                    )}
                                    {hotkeyEntry(
                                        "hide",
                                        t("ui_settings_hotkey_hide"),
                                        ["hotkey_hide"],
                                        "overlay_hide",
                                    )}
                                    {hotkeyEntry(
                                        "newer",
                                        t(
                                            "ui_settings_hotkey_show_newer_replay",
                                        ),
                                        ["hotkey_newer"],
                                        "overlay_newer",
                                    )}
                                    {hotkeyEntry(
                                        "older",
                                        t(
                                            "ui_settings_hotkey_show_older_replay",
                                        ),
                                        ["hotkey_older"],
                                        "overlay_older",
                                    )}
                                    {hotkeyEntry(
                                        "winrates",
                                        t(
                                            "ui_settings_hotkey_show_player_info",
                                        ),
                                        ["hotkey_winrates"],
                                        "overlay_player_info",
                                    )}
                                </Grid>
                            </div>
                            <div className="main-settings-group">
                                <h3 className="main-settings-group-title">
                                    {t("ui_settings_customize_colors")}
                                </h3>
                                {colorField(t("ui_settings_player_1"), [
                                    "color_player1",
                                ])}
                                {colorField(t("ui_settings_player_2"), [
                                    "color_player2",
                                ])}
                                {colorField(t("ui_settings_amon"), [
                                    "color_amon",
                                ])}
                                {colorField(t("ui_settings_mastery"), [
                                    "color_mastery",
                                ])}
                            </div>
                        </div>
                    </div>
                </Grid>
                <Grid size={4}>
                    <div className="main-settings-box main-settings-bottom">
                        <div className="main-settings-box main-bottom-left">
                            <button
                                type="button"
                                className="button-normal"
                                onClick={actions.overlayScreenshot}
                                disabled={actions.isBusy}
                            >
                                {t("ui_settings_overlay_screenshot")}
                            </button>
                            <button
                                type="button"
                                className="button-normal"
                                onClick={actions.parseReplayPrompt}
                                disabled={actions.isBusy}
                            >
                                {t("ui_settings_parse_replay")}
                            </button>
                            <button
                                type="button"
                                className="button-normal"
                                onClick={actions.createDesktopShortcut}
                                disabled={actions.isBusy}
                            >
                                {t("ui_settings_create_desktop_shortcut")}
                            </button>
                            <button
                                type="button"
                                className="button-normal"
                                onClick={checkUpdate}
                            >
                                {t("ui_settings_check_for_update")}
                            </button>
                        </div>
                        <div className="main-settings-box main-bottom-right">
                            <button
                                type="button"
                                className="button-normal"
                                onClick={actions.resetMainSettings}
                                disabled={
                                    actions.isBusy || !actions.hasPendingChanges
                                }
                            >
                                {t("ui_settings_reset")}
                            </button>
                            <button
                                type="button"
                                className="button-normal"
                                onClick={actions.applyMainSettings}
                                disabled={
                                    actions.isBusy || !actions.hasPendingChanges
                                }
                            >
                                {t("ui_settings_apply")}
                            </button>
                        </div>
                    </div>
                </Grid>
            </Grid>
        </div>
    );
}
