import * as React from "react";
import { emit } from "@tauri-apps/api/event";
import type {
    AppSettings,
    ConfigPayload,
    MonitorOption,
    OverlayColorPreviewPayload,
    OverlayLanguagePreviewPayload,
    OverlayRandomizerCatalog,
} from "../../../bindings/overlay";
import { loadConfigRequest, updateConfigRequest } from "../configApi";
import { cloneJson, getAtPath, setAtPath } from "../configValueUtils";
import type { JsonValue } from "../types";

const SCO_OVERLAY_COLOR_PREVIEW_EVENT = "sco://overlay-color-preview";
const SCO_OVERLAY_LANGUAGE_PREVIEW_EVENT = "sco://overlay-language-preview";

type QueuedLiveApply = {
    settings: AppSettings;
    requestSeq: number;
    successMessage: string;
};

type UseConfigSettingsArgs = {
    onThemeModeChange: (darkThemeEnabled: boolean) => void;
    setIsBusy: React.Dispatch<React.SetStateAction<boolean>>;
};

type UseConfigSettingsResult = {
    applyRuntimeSettings: (
        nextSettings: AppSettings,
        successMessage?: string,
    ) => Promise<ConfigPayload | null>;
    cancelPendingLiveApply: () => void;
    dirty: boolean;
    draft: AppSettings | null;
    draftRef: React.MutableRefObject<AppSettings | null>;
    loadSettings: () => Promise<void>;
    monitorCatalog: Array<MonitorOption>;
    randomizerCatalog: OverlayRandomizerCatalog | null;
    replaceDraft: (nextDraft: AppSettings | null) => void;
    resetSettings: () => void;
    safeStatus: (message: string) => void;
    saveProvidedSettings: (nextSettings: AppSettings) => Promise<void>;
    saveSettings: () => Promise<void>;
    setDraft: React.Dispatch<React.SetStateAction<AppSettings | null>>;
    setMonitorCatalog: React.Dispatch<
        React.SetStateAction<Array<MonitorOption>>
    >;
    setRandomizerCatalog: React.Dispatch<
        React.SetStateAction<OverlayRandomizerCatalog | null>
    >;
    setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
    settings: AppSettings | null;
    settingsMutationRef: React.MutableRefObject<Promise<void>>;
    setStatus: React.Dispatch<React.SetStateAction<string>>;
    status: string;
    updateField: (path: string[], value: JsonValue) => void;
};

function emitOverlayColorPreview(nextSettings: AppSettings): void {
    void (async () => {
        try {
            await emit<OverlayColorPreviewPayload>(
                SCO_OVERLAY_COLOR_PREVIEW_EVENT,
                {
                    color_player1:
                        typeof getAtPath(nextSettings, ["color_player1"]) ===
                        "string"
                            ? String(getAtPath(nextSettings, ["color_player1"]))
                            : undefined,
                    color_player2:
                        typeof getAtPath(nextSettings, ["color_player2"]) ===
                        "string"
                            ? String(getAtPath(nextSettings, ["color_player2"]))
                            : undefined,
                    color_amon:
                        typeof getAtPath(nextSettings, ["color_amon"]) ===
                        "string"
                            ? String(getAtPath(nextSettings, ["color_amon"]))
                            : undefined,
                    color_mastery:
                        typeof getAtPath(nextSettings, ["color_mastery"]) ===
                        "string"
                            ? String(getAtPath(nextSettings, ["color_mastery"]))
                            : undefined,
                },
            );
        } catch (error) {
            console.warn("Failed to emit overlay color preview", error);
        }
    })();
}

function emitOverlayLanguagePreview(nextSettings: AppSettings): void {
    void (async () => {
        try {
            await emit<OverlayLanguagePreviewPayload>(
                SCO_OVERLAY_LANGUAGE_PREVIEW_EVENT,
                {
                    language: String(
                        getAtPath(nextSettings, ["language"]) || "",
                    ),
                },
            );
        } catch (error) {
            console.warn("Failed to emit overlay language preview", error);
        }
    })();
}

function isColorSettingPath(path: string[]): boolean {
    return (
        path.length === 1 &&
        (path[0] === "color_player1" ||
            path[0] === "color_player2" ||
            path[0] === "color_amon" ||
            path[0] === "color_mastery")
    );
}

export function useConfigSettings({
    onThemeModeChange,
    setIsBusy,
}: UseConfigSettingsArgs): UseConfigSettingsResult {
    const [settings, setSettings] = React.useState<AppSettings | null>(null);
    const [draft, setDraft] = React.useState<AppSettings | null>(null);
    const [status, setStatus] = React.useState("Loading settings...");
    const [randomizerCatalog, setRandomizerCatalog] =
        React.useState<OverlayRandomizerCatalog | null>(null);
    const [monitorCatalog, setMonitorCatalog] = React.useState<
        Array<MonitorOption>
    >([]);
    const draftRef = React.useRef<AppSettings | null>(null);
    const settingsMutationRef = React.useRef<Promise<void>>(Promise.resolve());
    const latestLiveApplySeqRef = React.useRef<number>(0);
    const liveApplyInFlightRef = React.useRef<boolean>(false);
    const queuedLiveApplyRef = React.useRef<QueuedLiveApply | null>(null);
    draftRef.current = draft;

    const dirty = React.useMemo(() => {
        if (settings === null || draft === null) {
            return false;
        }
        return JSON.stringify(settings) !== JSON.stringify(draft);
    }, [settings, draft]);

    function safeStatus(message: string): void {
        console.log("[SCO/ui] status", message);
        setStatus(message);
    }

    function replaceDraft(nextDraft: AppSettings | null): void {
        if (
            nextDraft &&
            typeof nextDraft === "object" &&
            "dark_theme" in nextDraft
        ) {
            onThemeModeChange(Boolean(nextDraft.dark_theme));
        }
        draftRef.current = nextDraft;
        setDraft(nextDraft);
    }

    function queueSettingsMutation(task: () => Promise<void>): Promise<void> {
        const run = settingsMutationRef.current.then(task, task);
        settingsMutationRef.current = run.then(
            () => undefined,
            () => undefined,
        );
        return run;
    }

    function cancelPendingLiveApply(): void {
        queuedLiveApplyRef.current = null;
    }

    function performRuntimeSettingsApply(
        nextSettings: AppSettings,
        requestSeq: number,
        successMessage = "Changes applied immediately. Click Save to persist.",
    ): Promise<ConfigPayload | null> {
        liveApplyInFlightRef.current = true;
        return updateConfigRequest(nextSettings, false)
            .then((payload) => {
                setRandomizerCatalog(
                    (current) => payload.randomizer_catalog ?? current,
                );
                setMonitorCatalog(payload.monitor_catalog || []);
                if (requestSeq === latestLiveApplySeqRef.current) {
                    safeStatus(successMessage);
                }
                return payload;
            })
            .catch((error) => {
                if (requestSeq === latestLiveApplySeqRef.current) {
                    safeStatus(`Failed to apply changes: ${error.message}`);
                }
                return null;
            })
            .finally(() => {
                liveApplyInFlightRef.current = false;
                const queuedApply = queuedLiveApplyRef.current;
                if (
                    queuedApply !== null &&
                    queuedApply.requestSeq > requestSeq
                ) {
                    queuedLiveApplyRef.current = null;
                    void performRuntimeSettingsApply(
                        queuedApply.settings,
                        queuedApply.requestSeq,
                        queuedApply.successMessage,
                    );
                }
            });
    }

    function applyRuntimeSettings(
        nextSettings: AppSettings,
        successMessage = "Changes applied immediately. Click Save to persist.",
    ): Promise<ConfigPayload | null> {
        const requestSeq = latestLiveApplySeqRef.current + 1;
        latestLiveApplySeqRef.current = requestSeq;
        if (liveApplyInFlightRef.current) {
            queuedLiveApplyRef.current = {
                settings: nextSettings,
                requestSeq,
                successMessage,
            };
            return Promise.resolve(null);
        }
        return performRuntimeSettingsApply(
            nextSettings,
            requestSeq,
            successMessage,
        );
    }

    async function loadSettings(): Promise<void> {
        try {
            cancelPendingLiveApply();
            setIsBusy(true);
            const payload = await loadConfigRequest();
            if (!payload.settings) {
                throw new Error("Invalid response from API");
            }
            const activeSettings = payload.active_settings || payload.settings;
            setSettings(payload.settings);
            replaceDraft(activeSettings);
            setRandomizerCatalog(payload.randomizer_catalog ?? null);
            setMonitorCatalog(payload.monitor_catalog || []);
            setStatus("Settings loaded");
        } catch (error) {
            setStatus(`Failed to load settings: ${error.message}`);
        } finally {
            setIsBusy(false);
        }
    }

    async function saveProvidedSettings(
        nextSettings: AppSettings,
    ): Promise<void> {
        cancelPendingLiveApply();
        await queueSettingsMutation(async () => {
            try {
                setIsBusy(true);
                const payload = await updateConfigRequest(nextSettings, true);
                const activeSettings =
                    payload.active_settings || payload.settings;
                setSettings(payload.settings);
                replaceDraft(activeSettings);
                setRandomizerCatalog(
                    (current) => payload.randomizer_catalog ?? current,
                );
                setMonitorCatalog(payload.monitor_catalog || []);
                setStatus("Saved to settings.json");
            } catch (error) {
                setStatus(`Failed to save: ${error.message}`);
            } finally {
                setIsBusy(false);
            }
        });
    }

    async function saveSettings(): Promise<void> {
        if (draftRef.current === null) {
            return;
        }
        await saveProvidedSettings(draftRef.current);
    }

    function resetSettings(): void {
        if (settings !== null) {
            const nextDraft = cloneJson(settings);
            replaceDraft(nextDraft);
            cancelPendingLiveApply();
            emitOverlayColorPreview(nextDraft);
            void applyRuntimeSettings(nextDraft, "Reverted to saved settings.");
        }
    }

    function updateField(path: string[], value: JsonValue): void {
        if (draftRef.current === null) {
            return;
        }
        const nextDraft = setAtPath(draftRef.current, path, value);
        replaceDraft(nextDraft);
        if (isColorSettingPath(path)) {
            emitOverlayColorPreview(nextDraft);
        }
        if (path.length === 1 && path[0] === "language") {
            emitOverlayLanguagePreview(nextDraft);
        }
        cancelPendingLiveApply();
        void applyRuntimeSettings(nextDraft);
    }

    return {
        applyRuntimeSettings,
        cancelPendingLiveApply,
        dirty,
        draft,
        draftRef,
        loadSettings,
        monitorCatalog,
        randomizerCatalog,
        replaceDraft,
        resetSettings,
        safeStatus,
        saveProvidedSettings,
        saveSettings,
        setDraft,
        setMonitorCatalog,
        setRandomizerCatalog,
        setSettings,
        settings,
        settingsMutationRef,
        setStatus,
        status,
        updateField,
    };
}
