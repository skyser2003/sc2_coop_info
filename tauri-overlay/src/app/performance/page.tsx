import { useEffect, useState } from "react";
import type { JsonObject, JsonValue } from "../config/types";

import styles from "./page.module.css";

type CpuUsageLevel = "low" | "normal" | "high";

type CpuUsageRow = {
    label: string;
    value: string;
    level: CpuUsageLevel;
};

type PerformanceOverlayPayload = {
    processTitle: string;
    sc2Ram: string;
    sc2Read: string;
    sc2ReadTotal: string;
    sc2Write: string;
    sc2WriteTotal: string;
    sc2Cpu: string;
    sc2CpuLevel: CpuUsageLevel;
    systemRam: string;
    systemRamLevel: CpuUsageLevel;
    systemDown: string;
    systemDownTotal: string;
    systemUp: string;
    systemUpTotal: string;
    cpuTotal: string;
    cpuTotalLevel: CpuUsageLevel;
    cpuCores: CpuUsageRow[];
};

type PerformanceRuntimeWindow = typeof window & {
    __TAURI_INTERNALS__?: {
        invoke?: (command: string, args?: JsonObject) => Promise<JsonValue>;
        core?: {
            invoke?: (command: string, args?: JsonObject) => Promise<JsonValue>;
        };
    };
    __TAURI__?: {
        core?: {
            invoke?: (command: string, args?: JsonObject) => Promise<JsonValue>;
        };
    };
    updatePerformanceStats?: (payload: PerformanceOverlayPayload) => void;
    setPerformanceEditMode?: (enabled: boolean) => void;
};

const DEFAULT_STATS: PerformanceOverlayPayload = {
    processTitle: "StarCraft II",
    sc2Ram: "-",
    sc2Read: "-",
    sc2ReadTotal: "-",
    sc2Write: "-",
    sc2WriteTotal: "-",
    sc2Cpu: "-",
    sc2CpuLevel: "normal",
    systemRam: "-",
    systemRamLevel: "normal",
    systemDown: "-",
    systemDownTotal: "-",
    systemUp: "-",
    systemUpTotal: "-",
    cpuTotal: "-",
    cpuTotalLevel: "normal",
    cpuCores: [],
};

function levelClass(level: CpuUsageLevel): string {
    if (level === "high") {
        return styles.isHigh;
    }
    if (level === "low") {
        return styles.isLow;
    }
    return "";
}

function optionalClassName(
    baseClassName: string,
    optionalClassName: string,
): string {
    if (optionalClassName.length === 0) {
        return baseClassName;
    }
    return `${baseClassName} ${optionalClassName}`;
}

async function startPerformanceDrag(): Promise<void> {
    const runtime = window as PerformanceRuntimeWindow;
    const invoke =
        runtime.__TAURI_INTERNALS__?.invoke ??
        runtime.__TAURI_INTERNALS__?.core?.invoke ??
        runtime.__TAURI__?.core?.invoke;
    if (typeof invoke !== "function") {
        return;
    }
    await invoke("performance_start_drag");
}

export default function PerformancePage() {
    const [stats, setStats] =
        useState<PerformanceOverlayPayload>(DEFAULT_STATS);
    const [editMode, setEditMode] = useState(false);

    useEffect(() => {
        const root = document.documentElement;
        const body = document.body;
        root.style.background = "transparent";
        body.style.background = "transparent";
        body.style.margin = "0";
        body.style.overflow = "hidden";

        const runtime = window as PerformanceRuntimeWindow;
        runtime.updatePerformanceStats = (
            payload: PerformanceOverlayPayload,
        ) => {
            setStats(payload);
        };
        runtime.setPerformanceEditMode = (enabled: boolean) => {
            setEditMode(Boolean(enabled));
        };

        return () => {
            delete runtime.updatePerformanceStats;
            delete runtime.setPerformanceEditMode;
        };
    }, []);

    return (
        <main className={styles.performanceOverlayRoot}>
            <div
                className={optionalClassName(
                    `${styles.performanceDragbar} performance-dragbar`,
                    editMode ? styles.isVisible : "",
                )}
                data-tauri-drag-region
                onMouseDown={() => {
                    void startPerformanceDrag();
                }}
            >
                Drag performance overlay
            </div>
            <section className={styles.performanceCard}>
                <div className={styles.performanceColumns}>
                    <section className={styles.performanceColumn}>
                        <h1>{stats.processTitle}</h1>
                        <div className={styles.performanceStatGrid}>
                            <span className={styles.performanceLabel}>RAM</span>
                            <span className={styles.performanceValue}>
                                {stats.sc2Ram}
                            </span>
                            <span className={styles.performanceLabel}>
                                Read
                            </span>
                            <span className={styles.performanceValue}>
                                {stats.sc2Read}
                            </span>
                            <span className={styles.performanceSpacer} />
                            <span className={styles.performanceValue}>
                                {stats.sc2ReadTotal}
                            </span>
                            <span className={styles.performanceLabel}>
                                Write
                            </span>
                            <span className={styles.performanceValue}>
                                {stats.sc2Write}
                            </span>
                            <span className={styles.performanceSpacer} />
                            <span className={styles.performanceValue}>
                                {stats.sc2WriteTotal}
                            </span>
                        </div>
                        <div className={styles.performanceProcessCpu}>
                            <span className={styles.performanceSectionTitle}>
                                CPUc
                            </span>
                            <span
                                className={optionalClassName(
                                    styles.performanceValue,
                                    levelClass(stats.sc2CpuLevel),
                                )}
                            >
                                {stats.sc2Cpu}
                            </span>
                        </div>
                    </section>

                    <section className={styles.performanceColumn}>
                        <h1>System</h1>
                        <div className={styles.performanceStatGrid}>
                            <span className={styles.performanceLabel}>RAM</span>
                            <span
                                className={optionalClassName(
                                    styles.performanceValue,
                                    levelClass(stats.systemRamLevel),
                                )}
                            >
                                {stats.systemRam}
                            </span>
                            <span className={styles.performanceLabel}>
                                Down
                            </span>
                            <span className={styles.performanceValue}>
                                {stats.systemDown}
                            </span>
                            <span className={styles.performanceSpacer} />
                            <span className={styles.performanceValue}>
                                {stats.systemDownTotal}
                            </span>
                            <span className={styles.performanceLabel}>
                                Upload
                            </span>
                            <span className={styles.performanceValue}>
                                {stats.systemUp}
                            </span>
                            <span className={styles.performanceSpacer} />
                            <span className={styles.performanceValue}>
                                {stats.systemUpTotal}
                            </span>
                        </div>
                        <div className={styles.performanceCpuList}>
                            <h2>CPU utilization</h2>
                            {stats.cpuCores.map((entry) => (
                                <div
                                    className={styles.performanceCpuRow}
                                    key={entry.label}
                                >
                                    <span
                                        className={styles.performanceCpuLabel}
                                    >
                                        {entry.label}
                                    </span>
                                    <span
                                        className={optionalClassName(
                                            styles.performanceCpuValue,
                                            levelClass(entry.level),
                                        )}
                                    >
                                        {entry.value}
                                    </span>
                                </div>
                            ))}
                            <div
                                className={`${styles.performanceCpuRow} ${styles.performanceCpuTotal}`}
                            >
                                <span className={styles.performanceCpuLabel}>
                                    total
                                </span>
                                <span
                                    className={optionalClassName(
                                        styles.performanceCpuValue,
                                        levelClass(stats.cpuTotalLevel),
                                    )}
                                >
                                    {stats.cpuTotal}
                                </span>
                            </div>
                        </div>
                    </section>
                </div>
            </section>
        </main>
    );
}
