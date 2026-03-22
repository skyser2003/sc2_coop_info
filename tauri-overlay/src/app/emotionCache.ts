import createCache from "@emotion/cache";

const EMOTION_NONCE = "sco-overlay-mui";

function resolveInsertionPoint(): HTMLElement | undefined {
    if (typeof document === "undefined") {
        return undefined;
    }
    const element = document.querySelector<HTMLMetaElement>(
        'meta[name="emotion-insertion-point"]',
    );
    return element ?? undefined;
}

export function createEmotionCache() {
    return createCache({
        key: "mui",
        nonce: EMOTION_NONCE,
        prepend: true,
        insertionPoint: resolveInsertionPoint(),
    });
}

export { EMOTION_NONCE };
