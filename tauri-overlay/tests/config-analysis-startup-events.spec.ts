import { expect, test, type Page } from "@playwright/test";
import type {
    ReplayScanProgressPayload,
    StatsAnalysisPayload,
    StatsStatePayload,
} from "../src/bindings/overlay";
import { installConfigMock } from "./helpers/config-mock";

const emptyAnalysis: StatsAnalysisPayload = {
    MapData: {},
    CommanderData: {},
    AllyCommanderData: {},
    DifficultyData: {},
    RegionData: {},
    PlayerData: {},
    AmonData: {},
    MapDataReady: true,
    UnitData: {
        main: {},
        ally: {},
        amon: {},
    },
};

function progressPayload(
    stage: string,
    status: string,
    total: number,
    completed: number,
): ReplayScanProgressPayload {
    return {
        stage,
        status,
        parsing_status: status,
        total,
        total_replay_files: total,
        cache_hits: 0,
        files_already_cached: 0,
        to_parse: Math.max(total - completed, 0),
        completed,
        newly_parsed: completed,
        newly_parsed_files: completed,
        failed: 0,
        parse_failed_files: 0,
        parse_skipped: 0,
        parse_skipped_files: 0,
        elapsed_ms: completed * 100,
        total_time_taken_ms: completed * 100,
    };
}

function statsPayload(input: {
    analysisRunning: boolean;
    analysisRunningMode?: string;
    detailedParsedCount: number;
    detailedStatus: string;
    games: number;
    message: string;
    progress: ReplayScanProgressPayload;
    simpleStatus: string;
    totalValidFiles: number;
}): StatsStatePayload {
    return {
        ready: !input.analysisRunning,
        games: input.games,
        detailed_parsed_count: input.detailedParsedCount,
        total_valid_files: input.totalValidFiles,
        analysis: emptyAnalysis,
        main_players: [],
        main_handles: [],
        analysis_running: input.analysisRunning,
        ...(input.analysisRunningMode
            ? { analysis_running_mode: input.analysisRunningMode }
            : {}),
        simple_analysis_status: input.simpleStatus,
        detailed_analysis_status: input.detailedStatus,
        detailed_analysis_atstart: false,
        prestige_names: {},
        message: input.message,
        scan_progress: input.progress,
        query: "",
    };
}

function detailedRunningStats(
    progress: ReplayScanProgressPayload,
): StatsStatePayload {
    return statsPayload({
        analysisRunning: true,
        analysisRunningMode: "detailed",
        detailedParsedCount: 2,
        detailedStatus: "Detailed analysis: generating cache.",
        games: 10,
        message: "Detailed analysis: generating cache.",
        progress,
        simpleStatus: "Simple analysis: completed.",
        totalValidFiles: 10,
    });
}

function simpleRunningStats(
    progress: ReplayScanProgressPayload,
): StatsStatePayload {
    return statsPayload({
        analysisRunning: true,
        analysisRunningMode: "simple",
        detailedParsedCount: 0,
        detailedStatus: "Detailed analysis: not started.",
        games: 0,
        message: "Simple analysis: scanning replays.",
        progress,
        simpleStatus: "Simple analysis: scanning replays.",
        totalValidFiles: 0,
    });
}

function completedStats(detailedParsedCount: number): StatsStatePayload {
    return statsPayload({
        analysisRunning: false,
        detailedParsedCount,
        detailedStatus: `Detailed analysis: loaded from cache (${detailedParsedCount}/10).`,
        games: 10,
        message: "Detailed analysis cache generation completed.",
        progress: progressPayload("analysis_ready", "Completed", 10, 10),
        simpleStatus: "Simple analysis: completed.",
        totalValidFiles: 10,
    });
}

async function statsRequestCount(page: Page): Promise<number> {
    return page.evaluate(() => window.__SCO_STATS_REQUESTS__.length);
}

async function setMockStats(
    page: Page,
    payload: StatsStatePayload,
): Promise<void> {
    await page.evaluate((nextPayload) => {
        window.__setMockStatsPayload?.(nextPayload);
    }, payload);
}

async function emitConfigEvent(
    page: Page,
    eventName: string,
    payload: TestJsonValue,
): Promise<void> {
    const payloadJson = JSON.stringify(payload);
    await page.evaluate(
        (args: { eventName: string; payloadJson: string }) => {
            const eventPayload = JSON.parse(args.payloadJson) as TestJsonValue;
            window.__emitMockConfigEvent?.(args.eventName, eventPayload);
        },
        { eventName, payloadJson },
    );
}

test.describe("Config analysis startup events", () => {
    test("shows completed cached analysis when startup finishes before settings loads", async ({
        page,
    }) => {
        await installConfigMock(page, {
            stats: completedStats(8),
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });

        await expect(page.getByText("Progress: 8/10")).toBeVisible();
        expect(await statsRequestCount(page)).toBe(0);
    });

    test("refreshes settings stats when detailed analysis completes after frontend loads", async ({
        page,
    }) => {
        await installConfigMock(page, {
            stats: detailedRunningStats(
                progressPayload("detailed_analysis_running", "Parsing", 10, 2),
            ),
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });
        await expect(page.getByText("Progress: 2/10")).toBeVisible();

        const midProgress = progressPayload(
            "detailed_analysis_running",
            "Parsing",
            10,
            4,
        );
        await emitConfigEvent(page, "sco://replay-scan-progress", midProgress);
        await expect(page.getByText("Progress: 4/10")).toBeVisible();

        await setMockStats(page, completedStats(9));
        await emitConfigEvent(page, "sco://analysis-completed", {
            mode: "detailed",
            message: "Detailed analysis cache generation completed.",
        });

        await expect.poll(() => statsRequestCount(page)).toBeGreaterThan(0);
        await expect(page.getByText("Progress: 9/10")).toBeVisible();
        await expect(
            page.getByRole("button", { name: "Run detailed analysis" }),
        ).toBeEnabled();
    });

    test("uses live progress totals while simple analysis is running", async ({
        page,
    }) => {
        await installConfigMock(page, {
            stats: simpleRunningStats(
                progressPayload("scan_running", "Parsing", 10, 1),
            ),
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });
        await expect(page.getByText("Progress: 1/10")).toBeVisible();

        await emitConfigEvent(
            page,
            "sco://replay-scan-progress",
            progressPayload("scan_running", "Parsing", 10, 3),
        );

        await expect(page.getByText("Progress: 3/10")).toBeVisible();
    });
});
