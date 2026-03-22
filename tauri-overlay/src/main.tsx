import React, { StrictMode } from "react";
import * as ReactDOM from "react-dom/client";
import { CacheProvider } from "@emotion/react";
import Page from "./app/page";
import { createEmotionCache } from "./app/emotionCache";

declare global {
    interface Window {
        React: typeof React;
        ReactDOM: typeof ReactDOM;
    }
}

window.React = React;
window.ReactDOM = ReactDOM;

const emotionCache = createEmotionCache();

const root = ReactDOM.createRoot(
    document.getElementById("app") ?? document.body,
);
root.render(
    <StrictMode>
        <CacheProvider value={emotionCache}>
            <Page />
        </CacheProvider>
    </StrictMode>,
);
