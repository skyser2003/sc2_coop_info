import type {
    JsonObject,
    JsonPrimitive,
    JsonValue,
} from "../src/app/config/types";
import type { StatsStatePayload } from "../src/bindings/overlay";

export {};

declare global {
    type TestJsonPrimitive = JsonPrimitive;
    type TestJsonObject = Partial<JsonObject>;
    type TestJsonValue = JsonValue;
    type TestTauriRequest = TestJsonObject & {
        action?: string;
        body?: TestTauriRequest;
        command?: string;
        directory?: string;
        event?: string;
        eventId?: number;
        file?: string;
        handler?: number;
        limit?: number;
        method?: string;
        path?: string;
        persist?: boolean;
        query?: string;
        request?: TestTauriRequest;
        settings?: TestJsonObject;
    };
    type TestConfigRequestRecord = TestTauriRequest | null;

    interface Window {
        __TAURI_INTERNALS__: {};
        __TAURI_EVENT_PLUGIN_INTERNALS__: {
            unregisterListener?: (eventName: string, eventId: number) => void;
        };
        __emitMockConfigEvent?: (eventName: string, payload: JsonValue) => void;
        __setMockStatsPayload?: (payload: StatsStatePayload) => void;
        __SCO_ACTION_REQUESTS__: TestConfigRequestRecord[];
        __SCO_ANALYSIS_STATUS_REQUESTS__: TestConfigRequestRecord[];
        __SCO_CONFIG_GET_REQUESTS__: TestConfigRequestRecord[];
        __SCO_CONFIG_APPLY_REQUESTS__: TestJsonObject[];
        __SCO_CONFIG_SAVE_REQUESTS__: TestJsonObject[];
        __SCO_FOLDER_PICKER_REQUESTS__: TestConfigRequestRecord[];
        __SCO_STATS_ACTION_REQUESTS__: TestConfigRequestRecord[];
        __SCO_STATS_REQUESTS__: TestConfigRequestRecord[];
        __SCO_TAB_REQUESTS__: TestConfigRequestRecord[];
    }
}
