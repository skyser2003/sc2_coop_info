import * as React from "react";
import { listen } from "@tauri-apps/api/event";
import type { AnalysisStatusPayload } from "../../../bindings/overlay";
import { loadAnalysisStatusRequest } from "../configApi";

const SCO_ANALYSIS_STATUS_EVENT = "sco://analysis-status";

type UseConfigAnalysisStatusResult = {
    analysisStatus: AnalysisStatusPayload | null;
};

export function useConfigAnalysisStatus(
    frontendLoaded: boolean,
): UseConfigAnalysisStatusResult {
    const [analysisStatus, setAnalysisStatus] =
        React.useState<AnalysisStatusPayload | null>(null);

    React.useEffect(() => {
        if (!frontendLoaded) {
            return undefined;
        }

        let active = true;
        let receivedEvent = false;
        let unlisten: (() => void) | null = null;
        const loadInitialAnalysisStatus = async (): Promise<void> => {
            try {
                const currentStatus = await loadAnalysisStatusRequest();
                if (active && !receivedEvent) {
                    setAnalysisStatus(currentStatus);
                }
            } catch (error) {
                console.warn("Failed to load analysis status", error);
            }
        };

        void listen<AnalysisStatusPayload>(
            SCO_ANALYSIS_STATUS_EVENT,
            (event) => {
                receivedEvent = true;
                if (active) {
                    setAnalysisStatus(event.payload);
                }
            },
        )
            .then((listener) => {
                if (!active) {
                    listener();
                    return;
                }
                unlisten = listener;
                void loadInitialAnalysisStatus();
            })
            .catch((error: Error) => {
                console.warn("Failed to subscribe to analysis status", error);
            });

        return () => {
            active = false;
            if (unlisten) {
                unlisten();
            }
        };
    }, [frontendLoaded]);

    return { analysisStatus };
}
