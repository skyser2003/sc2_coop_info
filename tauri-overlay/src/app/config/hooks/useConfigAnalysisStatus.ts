import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import type { AnalysisStatusPayload } from "../../../bindings/overlay";
import { loadAnalysisStatusRequest } from "../configApi";

const SCO_REPLAY_SCAN_PROGRESS_EVENT = "sco://replay-scan-progress";
const SCO_ANALYSIS_COMPLETED_EVENT = "sco://analysis-completed";

type UseConfigAnalysisStatusResult = {
    analysisStatus: AnalysisStatusPayload | null;
    refreshAnalysisStatus: () => Promise<void>;
};

export function useConfigAnalysisStatus(
    frontendLoaded: boolean,
): UseConfigAnalysisStatusResult {
    const [analysisStatus, setAnalysisStatus] =
        React.useState<AnalysisStatusPayload | null>(null);

    const refreshAnalysisStatus = React.useCallback(async (): Promise<void> => {
        try {
            setAnalysisStatus(await loadAnalysisStatusRequest());
        } catch (error) {
            console.warn("Failed to load analysis status", error);
        }
    }, []);

    React.useEffect(() => {
        if (!frontendLoaded) {
            return undefined;
        }

        let active = true;
        let unlisteners: Array<() => void> = [];
        const updateAnalysisStatus = async (): Promise<void> => {
            try {
                const currentStatus = await loadAnalysisStatusRequest();
                if (active) {
                    setAnalysisStatus(currentStatus);
                }
            } catch (error) {
                console.warn("Failed to load analysis status", error);
            }
        };

        void Promise.all([
            listen(SCO_ANALYSIS_COMPLETED_EVENT, updateAnalysisStatus),
            listen(SCO_REPLAY_SCAN_PROGRESS_EVENT, updateAnalysisStatus),
        ])
            .then((listeners) => {
                if (!active) {
                    for (const unlisten of listeners) {
                        unlisten();
                    }
                    return;
                }
                unlisteners = listeners;
                void updateAnalysisStatus();
            })
            .catch((error: Error) => {
                console.warn("Failed to subscribe to analysis status", error);
            });

        return () => {
            active = false;
            for (const unlisten of unlisteners) {
                unlisten();
            }
        };
    }, [frontendLoaded]);

    return { analysisStatus, refreshAnalysisStatus };
}
