import { Chart, ChartConfiguration, registerables } from "chart.js";
import type { ReplayPlayerSeries } from "../../bindings/overlay";

const supportColor = "#aaa";
let player1Color = "#0080F8";
let player2Color = "#00D532";
let chartDevicePixelRatio = 0;

type ChartMetric = "army" | "supply" | "killed" | "mining";

export type ReplayDataRecord = Record<string, ReplayPlayerSeries>;
export type OverlayChartCanvasMap = Record<
    ChartMetric,
    HTMLCanvasElement | null
>;

const chartMetrics: ChartMetric[] = ["army", "supply", "killed", "mining"];

type OverlayChart = Chart<"line", number[], string>;

const charts: Record<ChartMetric, OverlayChart | null> = {
    army: null,
    supply: null,
    killed: null,
    mining: null,
};

let chartComponentsRegistered = false;

function ensureChartComponentsRegistered(): void {
    if (chartComponentsRegistered) {
        return;
    }
    Chart.register(...registerables);
    chartComponentsRegistered = true;
}

function getExistingChartForCanvas(
    canvas: HTMLCanvasElement,
): OverlayChart | null {
    const chart = Chart.getChart(canvas);
    if (!chart) {
        return null;
    }
    return chart as OverlayChart;
}

function destroyChartSafe(chart: OverlayChart | undefined | null): void {
    if (!chart) return;
    try {
        chart.destroy();
    } catch {
        // Ignore destroy races during route teardown.
    }
}

function getReplayPlayer(
    replayData: ReplayDataRecord,
    playerId: 1 | 2,
): ReplayPlayerSeries | null {
    const numberKey = replayData[playerId];
    if (numberKey) {
        return numberKey;
    }
    const stringKey = replayData[String(playerId)];
    return stringKey ?? null;
}

function chartTitle(metric: ChartMetric): string {
    if (metric === "army") return "Army value";
    if (metric === "supply") return "Supply used";
    if (metric === "killed") return "Kill count";
    return "Resource collection rate";
}

function generateConfig(
    player1: ReplayPlayerSeries,
    player2: ReplayPlayerSeries,
    labels: string[],
    metric: ChartMetric,
): ChartConfiguration<"line", number[], string> {
    return {
        type: "line",
        data: {
            labels,
            datasets: [
                {
                    data: player1[metric],
                    label: player1.name,
                    borderColor: player1Color,
                },
                {
                    data: player2[metric],
                    label: player2.name,
                    borderColor: player2Color,
                },
            ],
        },
        options: {
            responsive: true,
            devicePixelRatio:
                chartDevicePixelRatio > 0 ? chartDevicePixelRatio : undefined,
            events: [],
            datasets: {
                line: {
                    pointRadius: 0,
                },
            },
            plugins: {
                legend: {
                    display: false,
                    labels: {
                        color: "white",
                    },
                },
                title: {
                    display: true,
                    fullSize: false,
                    text: chartTitle(metric),
                    color: "#ddd",
                    font: {
                        family: "Eurostile",
                        size: 16,
                    },
                    padding: {
                        top: 10,
                        bottom: 7,
                    },
                },
            },
            scales: {
                x: {
                    display: true,
                    title: {
                        display: false,
                    },
                    ticks: {
                        color: supportColor,
                    },
                    border: {
                        color: supportColor,
                    },
                    grid: {
                        color: "#333",
                        tickColor: supportColor,
                    },
                },
                y: {
                    display: true,
                    border: {
                        color: supportColor,
                    },
                    grid: {
                        color: "#333",
                        tickColor: supportColor,
                    },
                    title: {
                        display: false,
                    },
                    ticks: {
                        color: supportColor,
                    },
                },
            },
        },
    };
}

function updateChart(
    chart: OverlayChart,
    player1: ReplayPlayerSeries,
    player2: ReplayPlayerSeries,
    labels: string[],
    metric: ChartMetric,
): void {
    chart.data.datasets[0].data = player1[metric];
    chart.data.datasets[1].data = player2[metric];
    chart.data.datasets[0].label = player1.name;
    chart.data.datasets[1].label = player2.name;
    chart.data.labels = labels;
    chart.update();
}

function plotChart(
    replayData: ReplayDataRecord,
    labels: string[],
    metric: ChartMetric,
    canvas: HTMLCanvasElement | null,
): void {
    if (!canvas) {
        return;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) {
        return;
    }

    const player1 = getReplayPlayer(replayData, 1);
    const player2 = getReplayPlayer(replayData, 2);
    if (!player1 || !player2) {
        return;
    }

    const tracked = charts[metric];
    if (tracked && tracked.canvas === canvas) {
        updateChart(tracked, player1, player2, labels, metric);
        return;
    }

    destroyChartSafe(tracked);
    charts[metric] = null;

    destroyChartSafe(getExistingChartForCanvas(canvas));
    ensureChartComponentsRegistered();
    charts[metric] = new Chart(
        ctx,
        generateConfig(player1, player2, labels, metric),
    );
}

export function setChartPlayerColors(p1: string, p2: string): void {
    player1Color = p1;
    player2Color = p2;
}

export function plot_charts(
    replayData: ReplayDataRecord,
    canvases: OverlayChartCanvasMap,
): void {
    const player1 = getReplayPlayer(replayData, 1);
    if (!player1) return;

    const labels: string[] = [];
    for (let i = 0; i < player1.army.length; i++) {
        labels.push(formatLength(i * 10, false));
    }

    for (const metric of chartMetrics) {
        plotChart(replayData, labels, metric, canvases[metric]);
    }
}

export function update_charts_colors(p1color: string, p2color: string): void {
    for (const metric of chartMetrics) {
        const chart = charts[metric];
        if (!chart) continue;
        chart.data.datasets[0].borderColor = p1color;
        chart.data.datasets[1].borderColor = p2color;
        chart.update();
    }
}

export function set_overlay_chart_pixel_ratio(ratio: number): void {
    chartDevicePixelRatio = ratio;
    for (const metric of chartMetrics) {
        const chart = charts[metric];
        if (!chart) {
            continue;
        }
        chart.options.devicePixelRatio = ratio;
        chart.resize();
        chart.update("none");
    }
}

export function reset_overlay_chart_pixel_ratio(): void {
    chartDevicePixelRatio = 0;
    for (const metric of chartMetrics) {
        const chart = charts[metric];
        if (!chart) {
            continue;
        }
        chart.options.devicePixelRatio = undefined;
        chart.resize();
        chart.update("none");
    }
}

export function destroy_overlay_charts(): void {
    for (const metric of chartMetrics) {
        destroyChartSafe(charts[metric]);
        charts[metric] = null;
    }
}

function formatLength(seconds: number, multiply = true): string {
    const gameSeconds = multiply
        ? Math.round(seconds * 1.4)
        : Math.round(seconds);

    let sec = gameSeconds % 60;
    let min = ((gameSeconds - sec) / 60) % 60;
    let hr = (gameSeconds - sec - min * 60) / 3600;

    const hrPrefix = hr > 0 ? `${hr}:` : "";
    const minPart = min === 0 ? "00:" : min < 10 ? `0${min}:` : `${min}:`;
    const secPart = sec < 10 ? `0${sec}` : `${sec}`;

    return `${hrPrefix}${minPart}${secPart}`;
}
