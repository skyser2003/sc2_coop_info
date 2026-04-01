import { useMemo, useState } from "react";
import { HashRouter, Navigate, Route, Routes } from "react-router-dom";
import { CssBaseline, ThemeProvider } from "@mui/material";
import ConfigPage from "./config/page";
import OverlayPage from "./overlay/page";
import PerformancePage from "./performance/page";
import {
    createConfigTheme,
    getConfigThemeMode,
    type ConfigThemeMode,
} from "./theme";

export default function Page() {
    const [configThemeMode, setConfigThemeMode] =
        useState<ConfigThemeMode>("dark");
    const theme = useMemo(
        () => createConfigTheme(configThemeMode),
        [configThemeMode],
    );

    return (
        <HashRouter>
            <Routes>
                <Route
                    path="/config/*"
                    element={
                        <ThemeProvider theme={theme}>
                            <CssBaseline />
                            <ConfigPage
                                onThemeModeChange={(darkThemeEnabled) => {
                                    setConfigThemeMode(
                                        getConfigThemeMode(darkThemeEnabled),
                                    );
                                }}
                            />
                        </ThemeProvider>
                    }
                />
                <Route path="/overlay" element={<OverlayPage />} />
                <Route path="/performance" element={<PerformancePage />} />
                <Route
                    path="/"
                    element={<Navigate to="/config/settings" replace />}
                />
                <Route
                    path="*"
                    element={<Navigate to="/config/settings" replace />}
                />
            </Routes>
        </HashRouter>
    );
}
