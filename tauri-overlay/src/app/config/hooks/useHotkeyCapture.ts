import * as React from "react";
import { postConfigActionRequest } from "../configApi";

type UseHotkeyCaptureArgs = {
    settingsMutationRef: React.MutableRefObject<Promise<void>>;
};

type UseHotkeyCaptureResult = {
    activeHotkeyPath: string;
    beginHotkeyCapture: (path: string) => Promise<void>;
    endHotkeyCapture: (path: string) => Promise<void>;
};

async function syncHotkeyReassign(
    currentPath: string,
    nextPath: string,
): Promise<void> {
    if (currentPath === nextPath) {
        return;
    }

    try {
        if (currentPath !== "") {
            await postConfigActionRequest("hotkey_reassign_end", {
                path: currentPath,
            });
        }
        if (nextPath !== "") {
            await postConfigActionRequest("hotkey_reassign_begin", {
                path: nextPath,
            });
        }
    } catch (error) {
        console.warn("Failed to sync hotkey reassign state", error);
    }
}

export function useHotkeyCapture({
    settingsMutationRef,
}: UseHotkeyCaptureArgs): UseHotkeyCaptureResult {
    const [activeHotkeyPath, setActiveHotkeyPath] = React.useState("");
    const activeHotkeyPathRef = React.useRef<string>("");
    const hotkeyTransitionRef = React.useRef<Promise<void>>(Promise.resolve());
    activeHotkeyPathRef.current = activeHotkeyPath;

    React.useEffect(() => {
        return () => {
            if (activeHotkeyPathRef.current !== "") {
                hotkeyTransitionRef.current = hotkeyTransitionRef.current
                    .then(() =>
                        syncHotkeyReassign(activeHotkeyPathRef.current, ""),
                    )
                    .catch((error) => {
                        console.warn(
                            "Failed to clean up hotkey reassign state",
                            error,
                        );
                    });
            }
        };
    }, []);

    function transitionHotkeyCapture(nextPath: string): Promise<void> {
        hotkeyTransitionRef.current = hotkeyTransitionRef.current
            .then(async () => {
                await settingsMutationRef.current;
                const currentPath = activeHotkeyPathRef.current;
                if (currentPath === nextPath) {
                    return;
                }
                await syncHotkeyReassign(currentPath, nextPath);
                activeHotkeyPathRef.current = nextPath;
                setActiveHotkeyPath(nextPath);
            })
            .catch((error) => {
                console.warn("Failed to transition hotkey capture", error);
            });
        return hotkeyTransitionRef.current;
    }

    function beginHotkeyCapture(path: string): Promise<void> {
        return transitionHotkeyCapture(path);
    }

    function endHotkeyCapture(path: string): Promise<void> {
        if (activeHotkeyPathRef.current !== path) {
            return Promise.resolve();
        }
        return transitionHotkeyCapture("");
    }

    return {
        activeHotkeyPath,
        beginHotkeyCapture,
        endHotkeyCapture,
    };
}
