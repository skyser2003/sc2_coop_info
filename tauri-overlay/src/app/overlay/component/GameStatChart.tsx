import { useEffect, useRef, useState } from "react";
import {
    destroy_overlay_charts,
    OverlayChartCanvasMap,
    plot_charts,
    setChartPlayerColors,
    update_charts_colors,
    updateChartTitles,
} from "../charts";
import type {
    OverlayReplayPayload,
    ReplayPlayerSeries,
} from "../../../bindings/overlay";
import { LanguageManager } from "../../i18n/languageManager";

export interface ReplayChartVisible {
    visible: boolean;
    immediate: boolean;
}

type ChartStyleState = {
    display: "none" | "block";
    opacity: "0" | "1";
    transition: string;
};

const hiddenChartStyle: ChartStyleState = {
    display: "none",
    opacity: "0",
    transition: "",
};

function isReplayPlayerSeries(series: ReplayPlayerSeries): boolean {
    return (
        typeof series.name === "string" &&
        Array.isArray(series.army) &&
        Array.isArray(series.supply) &&
        Array.isArray(series.killed) &&
        Array.isArray(series.mining)
    );
}

function resolvePlayerSeriesByName(
    playerStats: OverlayReplayPayload["player_stats"],
    targetName: string,
    fallbackKey: "1" | "2",
): ReplayPlayerSeries | null {
    if (playerStats == null) {
        return null;
    }

    const trimmedTarget = targetName.trim();
    if (trimmedTarget !== "") {
        const matchedSeries = Object.values(playerStats).find(
            (series) =>
                isReplayPlayerSeries(series) &&
                series.name.trim() === trimmedTarget,
        );
        if (matchedSeries != null) {
            return matchedSeries;
        }
    }

    const fallbackSeries = playerStats[fallbackKey] ?? null;
    return fallbackSeries != null && isReplayPlayerSeries(fallbackSeries)
        ? fallbackSeries
        : null;
}

export default function GameStatChart({
    payload,
    chartVisibility,
    p1Color,
    p2Color,
    language,
    languageManager,
}: {
    payload: OverlayReplayPayload | null;
    chartVisibility: ReplayChartVisible;
    p1Color: string;
    p2Color: string;
    language: string;
    languageManager: LanguageManager;
}) {
    const [chartStyle, setChartStyle] =
        useState<ChartStyleState>(hiddenChartStyle);
    const armyChartRef = useRef<HTMLCanvasElement | null>(null);
    const supplyChartRef = useRef<HTMLCanvasElement | null>(null);
    const killedChartRef = useRef<HTMLCanvasElement | null>(null);
    const miningChartRef = useRef<HTMLCanvasElement | null>(null);

    useEffect(() => {
        const mainPlayerStats =
            payload?.mainPlayerStats ??
            resolvePlayerSeriesByName(
                payload?.player_stats,
                payload?.main ?? "",
                "1",
            );
        const allyPlayerStats =
            payload?.allyPlayerStats ??
            resolvePlayerSeriesByName(
                payload?.player_stats,
                payload?.ally ?? "",
                "2",
            );
        const shouldShowCharts =
            chartVisibility.visible &&
            mainPlayerStats !== null &&
            allyPlayerStats !== null;
        const chartCanvases: OverlayChartCanvasMap = {
            army: armyChartRef.current,
            supply: supplyChartRef.current,
            killed: killedChartRef.current,
            mining: miningChartRef.current,
        };

        const chartTitles = {
            army: languageManager.translate("ui_overlay_metric_army"),
            supply: languageManager.translate("ui_overlay_metric_supply"),
            killed: languageManager.translate("ui_overlay_metric_killed"),
            mining: languageManager.translate("ui_overlay_metric_mining"),
        };

        if (shouldShowCharts) {
            plot_charts(
                mainPlayerStats,
                allyPlayerStats,
                chartCanvases,
                chartTitles,
            );
            setChartStyle({
                display: "block",
                opacity: chartVisibility.immediate ? "1" : "0",
                transition: chartVisibility.immediate
                    ? "opacity 0s"
                    : "opacity 0.5s ease",
            });

            if (!chartVisibility.immediate) {
                const frameId = window.requestAnimationFrame(() => {
                    setChartStyle((previousStyle) => ({
                        ...previousStyle,
                        opacity: "1",
                    }));
                });

                return () => {
                    window.cancelAnimationFrame(frameId);
                };
            }

            return;
        }

        setChartStyle({
            display: chartVisibility.immediate ? "none" : "block",
            opacity: "0",
            transition: chartVisibility.immediate
                ? "opacity 0s"
                : "opacity 0.5s ease",
        });

        const destroyTimer = setTimeout(
            () => {
                destroy_overlay_charts();
                setChartStyle((previousStyle) => ({
                    ...previousStyle,
                    display: "none",
                }));
            },
            chartVisibility.immediate ? 0 : 500,
        );

        return () => {
            clearTimeout(destroyTimer);
        };
    }, [chartVisibility, payload]);

    useEffect(() => {
        const chartTitles = {
            army: languageManager.translate("ui_overlay_metric_army"),
            supply: languageManager.translate("ui_overlay_metric_supply"),
            killed: languageManager.translate("ui_overlay_metric_killed"),
            mining: languageManager.translate("ui_overlay_metric_mining"),
        };

        updateChartTitles(chartTitles);
    }, [language]);

    useEffect(() => {
        setChartPlayerColors(p1Color, p2Color);
        update_charts_colors(p1Color, p2Color);
    }, [p1Color, p2Color]);

    return (
        <div
            id="charts"
            style={{
                transition: chartStyle.transition,
                opacity: chartStyle.opacity,
                display: chartStyle.display,
            }}
        >
            <canvas id="armyChart" ref={armyChartRef} />
            <canvas id="supplyChart" ref={supplyChartRef} />
            <canvas id="killedChart" ref={killedChartRef} />
            <canvas id="miningChart" ref={miningChartRef} />
        </div>
    );
}
