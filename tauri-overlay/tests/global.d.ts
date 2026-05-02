import type {
    JsonObject,
    JsonPrimitive,
    JsonValue,
} from "../src/app/config/types";

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
        __TAURI_EVENT_PLUGIN_INTERNALS__: {};
        __SCO_ACTION_REQUESTS__: TestConfigRequestRecord[];
        __SCO_CONFIG_APPLY_REQUESTS__: TestJsonObject[];
        __SCO_CONFIG_SAVE_REQUESTS__: TestJsonObject[];
        __SCO_FOLDER_PICKER_REQUESTS__: TestConfigRequestRecord[];
        __SCO_TAB_REQUESTS__: TestConfigRequestRecord[];
    }
}
