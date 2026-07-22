import type { JsonValue } from "../types";

export type SettingsActions = {
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
    stopDetailedAnalysis: () => Promise<void> | void;
    deleteParsedData: () => Promise<void> | void;
    setLatestFirstWinBonusTime: (value: string) => Promise<void> | void;
    applyMainSettings: () => Promise<void> | void;
    resetMainSettings: () => void;
    isHotkeyClearKey: (key: string) => boolean;
    isHotkeyModifierKey: (key: string) => boolean;
    analysisRunning?: boolean;
    analysisRunningMode?: string | null;
    analysisStatus?: string;
    analysisScanProgress?: Record<string, JsonValue> | null;
    analysisTotalValidFiles?: number;
    analysisDetailedParsedCount?: number;
    monitorOptions?: Array<{
        index: number;
        label: string;
    }>;
};

export type SettingsValueReader = (
    path: string[],
    fallback?: JsonValue | null,
) => JsonValue | null;
