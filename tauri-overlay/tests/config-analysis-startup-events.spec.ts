import { expect, test, type Page } from "@playwright/test";
import type {
    AnalysisStatusPayload,
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

function completedStats(
    detailedParsedCount: number,
    simpleStatus = "Simple analysis: completed.",
): StatsStatePayload {
    return statsPayload({
        analysisRunning: false,
        detailedParsedCount,
        detailedStatus: `Detailed analysis: loaded from cache (${detailedParsedCount}/10).`,
        games: 10,
        message: "Detailed analysis cache generation completed.",
        progress: progressPayload("analysis_ready", "Completed", 10, 10),
        simpleStatus,
        totalValidFiles: 10,
    });
}

function analysisStatusPayload(
    stats: StatsStatePayload,
): AnalysisStatusPayload {
    return {
        status: "ok",
        ready: stats.ready,
        analysis_running: stats.analysis_running,
        ...(typeof stats.analysis_running_mode === "string"
            ? { analysis_running_mode: stats.analysis_running_mode }
            : {}),
        current_status:
            stats.analysis_running_mode === "simple"
                ? stats.simple_analysis_status
                : stats.detailed_analysis_status,
        simple_analysis_status: stats.simple_analysis_status,
        detailed_analysis_status: stats.detailed_analysis_status,
        detailed_parsed_count: stats.detailed_parsed_count,
        total_valid_files: stats.total_valid_files,
        scan_progress: stats.scan_progress,
    };
}

async function statsRequestCount(page: Page): Promise<number> {
    return page.evaluate(() => window.__SCO_STATS_REQUESTS__.length);
}

async function statsActionRequestCount(page: Page): Promise<number> {
    return page.evaluate(() => window.__SCO_STATS_ACTION_REQUESTS__.length);
}

async function analysisStatusRequestCount(page: Page): Promise<number> {
    return page.evaluate(() => window.__SCO_ANALYSIS_STATUS_REQUESTS__.length);
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

test.describe("Config statistics lazy loading", () => {
    test("loads completed analysis status without loading statistics on startup", async ({
        page,
    }) => {
        await installConfigMock(page, {
            stats: completedStats(8, "Simple analysis: waiting for startup."),
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });

        await expect(page.locator("#app-status")).toHaveText("Settings loaded");
        await expect.poll(() => analysisStatusRequestCount(page)).toBe(1);
        await expect(
            page.getByText("Detailed analysis: loaded from cache (8/10).", {
                exact: true,
            }),
        ).toBeVisible();
        await expect(
            page.getByText("No statistics loaded.", { exact: true }),
        ).toHaveCount(0);
        await expect(
            page.getByText("Simple analysis: waiting for startup.", {
                exact: true,
            }),
        ).toHaveCount(0);
        expect(await statsRequestCount(page)).toBe(0);
        expect(await statsActionRequestCount(page)).toBe(0);
    });

    test("updates analysis status outside statistics without loading statistics", async ({
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
        await expect(page.locator("#app-status")).toHaveText("Settings loaded");
        await expect.poll(() => analysisStatusRequestCount(page)).toBe(1);

        const statusRequestsBeforeEvent =
            await analysisStatusRequestCount(page);
        for (const completedCount of [3, 4, 5]) {
            const running = detailedRunningStats(
                progressPayload(
                    "detailed_analysis_running",
                    "Parsing",
                    10,
                    completedCount,
                ),
            );
            await setMockStats(page, running);
            await emitConfigEvent(
                page,
                "sco://analysis-status",
                analysisStatusPayload(running),
            );
        }

        const completed = completedStats(9);
        await setMockStats(page, completed);
        await emitConfigEvent(
            page,
            "sco://analysis-status",
            analysisStatusPayload(completed),
        );

        await expect(
            page.getByText("Detailed analysis: loaded from cache (9/10).", {
                exact: true,
            }),
        ).toBeVisible();
        expect(await statsRequestCount(page)).toBe(0);
        expect(await statsActionRequestCount(page)).toBe(0);
        expect(await analysisStatusRequestCount(page)).toBe(
            statusRequestsBeforeEvent,
        );
        await expect(page.locator("#app-status")).toHaveText("Settings loaded");
    });

    test("loads current statistics when the statistics tab opens", async ({
        page,
    }) => {
        await installConfigMock(page, {
            stats: completedStats(9),
        });

        await page.goto("/#/config/settings", {
            waitUntil: "domcontentloaded",
        });
        expect(await statsRequestCount(page)).toBe(0);
        expect(await statsActionRequestCount(page)).toBe(0);

        await page.getByRole("tab", { name: "Statistics" }).click();

        await expect.poll(() => statsRequestCount(page)).toBeGreaterThan(0);
        await expect(
            page.getByRole("button", { name: "Dump Data" }),
        ).toBeEnabled();
        expect(await statsActionRequestCount(page)).toBe(0);

        await page.getByRole("tab", { name: "Settings" }).click();
        await expect(
            page.getByText("Detailed analysis: loaded from cache (9/10).", {
                exact: true,
            }),
        ).toBeVisible();
        await expect(
            page.getByText("Detailed analysis cache generation completed.", {
                exact: true,
            }),
        ).toHaveCount(0);
        await expect(
            page.getByText("No statistics loaded.", { exact: true }),
        ).toHaveCount(0);
    });

    test("refreshes statistics on completion without displaying its message", async ({
        page,
    }) => {
        await installConfigMock(page, {
            stats: detailedRunningStats(
                progressPayload("detailed_analysis_running", "Parsing", 10, 2),
            ),
        });

        await page.goto("/#/config/statistics", {
            waitUntil: "domcontentloaded",
        });
        await expect.poll(() => statsRequestCount(page)).toBeGreaterThan(0);
        await expect(
            page.getByRole("button", { name: "Dump Data" }),
        ).toBeDisabled();
        const requestsBeforeCompletion = await statsRequestCount(page);
        const statusRequestsBeforeCompletion =
            await analysisStatusRequestCount(page);

        const completed = completedStats(9);
        await setMockStats(page, completed);
        await emitConfigEvent(
            page,
            "sco://analysis-status",
            analysisStatusPayload(completed),
        );

        await expect
            .poll(() => statsRequestCount(page))
            .toBeGreaterThan(requestsBeforeCompletion);
        await expect(
            page.getByRole("button", { name: "Dump Data" }),
        ).toBeEnabled();
        await expect(page.locator("#app-status")).toHaveText("Settings loaded");
        await expect(
            page.getByText("Detailed analysis cache generation completed.", {
                exact: true,
            }),
        ).toHaveCount(0);
        expect(await analysisStatusRequestCount(page)).toBe(
            statusRequestsBeforeCompletion,
        );
    });
});
