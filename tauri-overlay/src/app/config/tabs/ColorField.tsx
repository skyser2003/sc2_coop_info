import * as React from "react";
import type { JsonValue } from "../types";
import styles from "../configStyles";
import {
    clamp,
    HEX_COLOR_PATTERN,
    normalizeHexColor,
} from "./settingsTabUtils";

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
        <div key={path.join(".")} ref={rootRef} className={styles.colorRow}>
            <span className={styles.colorRowLabel}>{label}</span>
            <div className={styles.colorRowInput}>
                <button
                    type="button"
                    className={styles.colorRowInputButton}
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
                        className={styles.colorRowSwatch}
                        aria-hidden="true"
                        style={{ backgroundColor: localColor }}
                    />
                </button>
            </div>
            <div className={styles.colorRowPopupAnchor}>
                {isOpen ? (
                    <div
                        className={styles.colorPickerPopup}
                        data-disabled={String(disabled)}
                    >
                        <input
                            type="text"
                            className={[styles.input, styles.colorRowText]
                                .filter(Boolean)
                                .join(" ")}
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
                        <div className={styles.colorWheelPicker}>
                            <div
                                ref={wheelRef}
                                className={styles.colorWheelRing}
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
                                    className={styles.colorWheelRingMarker}
                                    style={wheelMarkerStyle}
                                />
                            </div>
                            <div
                                ref={squareRef}
                                className={styles.colorWheelSquare}
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
                                    className={styles.colorWheelSquareMarker}
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

export default ColorField;
