import { useEffect, useState } from "react";

import "./page.css";

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
        invoke?: (
            command: string,
            args?: Record<string, unknown>,
        ) => Promise<unknown>;
        core?: {
            invoke?: (
                command: string,
                args?: Record<string, unknown>,
            ) => Promise<unknown>;
        };
    };
    __TAURI__?: {
        core?: {
            invoke?: (
                command: string,
                args?: Record<string, unknown>,
            ) => Promise<unknown>;
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
        return "is-high";
    }
    if (level === "low") {
        return "is-low";
    }
    return "";
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
        <main className="performance-overlay-root">
            <div
                className={`performance-dragbar${editMode ? " is-visible" : ""}`}
                data-tauri-drag-region
                onMouseDown={() => {
                    void startPerformanceDrag();
                }}
            >
                Drag performance overlay
            </div>
            <section className="performance-card">
                <div className="performance-columns">
                    <section className="performance-column">
                        <h1>{stats.processTitle}</h1>
                        <div className="performance-stat-grid">
                            <span className="performance-label">RAM</span>
                            <span className="performance-value">
                                {stats.sc2Ram}
                            </span>
                            <span className="performance-label">Read</span>
                            <span className="performance-value">
                                {stats.sc2Read}
                            </span>
                            <span className="performance-spacer" />
                            <span className="performance-value">
                                {stats.sc2ReadTotal}
                            </span>
                            <span className="performance-label">Write</span>
                            <span className="performance-value">
                                {stats.sc2Write}
                            </span>
                            <span className="performance-spacer" />
                            <span className="performance-value">
                                {stats.sc2WriteTotal}
                            </span>
                        </div>
                        <div className="performance-process-cpu">
                            <span className="performance-section-title">
                                CPUc
                            </span>
                            <span
                                className={`performance-value ${levelClass(stats.sc2CpuLevel)}`}
                            >
                                {stats.sc2Cpu}
                            </span>
                        </div>
                    </section>

                    <section className="performance-column">
                        <h1>System</h1>
                        <div className="performance-stat-grid">
                            <span className="performance-label">RAM</span>
                            <span
                                className={`performance-value ${levelClass(stats.systemRamLevel)}`}
                            >
                                {stats.systemRam}
                            </span>
                            <span className="performance-label">Down</span>
                            <span className="performance-value">
                                {stats.systemDown}
                            </span>
                            <span className="performance-spacer" />
                            <span className="performance-value">
                                {stats.systemDownTotal}
                            </span>
                            <span className="performance-label">Upload</span>
                            <span className="performance-value">
                                {stats.systemUp}
                            </span>
                            <span className="performance-spacer" />
                            <span className="performance-value">
                                {stats.systemUpTotal}
                            </span>
                        </div>
                        <div className="performance-cpu-list">
                            <h2>CPU utilization</h2>
                            {stats.cpuCores.map((entry) => (
                                <div
                                    className="performance-cpu-row"
                                    key={entry.label}
                                >
                                    <span className="performance-cpu-label">
                                        {entry.label}
                                    </span>
                                    <span
                                        className={`performance-cpu-value ${levelClass(entry.level)}`}
                                    >
                                        {entry.value}
                                    </span>
                                </div>
                            ))}
                            <div className="performance-cpu-row performance-cpu-total">
                                <span className="performance-cpu-label">
                                    total
                                </span>
                                <span
                                    className={`performance-cpu-value ${levelClass(stats.cpuTotalLevel)}`}
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
