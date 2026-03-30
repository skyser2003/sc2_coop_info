import { Container, Typography } from "@mui/material";
import { alpha } from "@mui/material/styles";
import ConfigRoute from "./ConfigRoute";
import styles from "./page.module.css";
import { app } from "@tauri-apps/api";
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type ConfigPageProps = {
    onThemeModeChange: (darkThemeEnabled: boolean) => void;
};

export default function ConfigPage({ onThemeModeChange }: ConfigPageProps) {
    const [appVersion, setAppVersion] = useState<string>("v0.0.0");
    const [isDev, setIsDev] = useState<boolean>(false);

    useEffect(() => {
        app.getVersion().then((version) => {
            setAppVersion(`${version}`);
        });

        invoke<boolean>("is_dev").then((isDevInvoke) => {
            setIsDev(isDevInvoke);
        });
    });

    return (
        <Container
            component="main"
            maxWidth="xl"
            disableGutters
            className={styles.configPage}
            sx={(theme) => {
                const isDark = theme.palette.mode === "dark";

                return {
                    px: 3,
                    py: 3,
                    color: "text.primary",
                    maxWidth: "1200px",
                    "--config-page-text": theme.palette.text.primary,
                    "--config-muted-text": theme.palette.text.secondary,
                    "--config-subtle-strong": isDark ? "#cbd5e1" : "#334155",
                    "--config-disabled-text": isDark ? "#94a3b8" : "#64748b",
                    "--config-link-text": isDark
                        ? theme.palette.primary.light
                        : theme.palette.primary.dark,
                    "--config-link-hover-text": isDark ? "#bfdbfe" : "#1e40af",
                    "--config-accent-main": theme.palette.primary.main,
                    "--config-accent-soft": alpha(
                        theme.palette.primary.main,
                        isDark ? 0.18 : 0.1,
                    ),
                    "--config-accent-soft-strong": alpha(
                        theme.palette.primary.main,
                        isDark ? 0.3 : 0.16,
                    ),
                    "--config-accent-border": alpha(
                        theme.palette.primary.main,
                        isDark ? 0.38 : 0.26,
                    ),
                    "--config-accent-selected-bg": isDark
                        ? "rgba(30, 41, 59, 0.75)"
                        : "rgba(219, 234, 254, 0.6)",
                    "--config-surface": theme.palette.background.paper,
                    "--config-surface-soft": isDark
                        ? "rgba(8, 15, 30, 0.48)"
                        : "#f8fbff",
                    "--config-surface-muted": isDark ? "#111827" : "#edf3fa",
                    "--config-panel-backdrop": isDark
                        ? "rgba(0, 0, 0, 0.16)"
                        : "#edf3fa",
                    "--config-input-bg": isDark ? "#0a1020" : "#f8fbff",
                    "--config-input-hover-bg": isDark ? "#101a31" : "#eef5fd",
                    "--config-preview-bg": isDark ? "#111827" : "#dbe7f5",
                    "--config-border": theme.palette.divider,
                    "--config-border-strong": isDark ? "#334155" : "#c7d4e5",
                    "--config-shadow": isDark
                        ? "0 6px 18px rgb(0 0 0 / 18%)"
                        : "0 12px 34px rgb(15 23 42 / 0.08)",
                    "--config-table-header-bg": isDark
                        ? "rgba(15, 23, 42, 0.85)"
                        : "#edf3fa",
                    "--config-tab-bg": isDark ? "#1a2236" : "#edf3fa",
                    "--config-tab-hover-bg": isDark ? "#273451" : "#e2ebf6",
                    "--config-tab-active-bg": isDark ? "#111827" : "#ffffff",
                    "--config-tab-text": isDark ? "#9fb2c7" : "#526277",
                    "--config-tab-hover-text": isDark ? "#e5e7eb" : "#0f172a",
                    "--config-tab-active-text": isDark
                        ? theme.palette.primary.light
                        : theme.palette.primary.dark,
                    "--config-chip-bg": isDark
                        ? "rgba(30, 41, 59, 0.9)"
                        : "#edf3fa",
                    "--config-chip-text": theme.palette.text.primary,
                    "--config-empty-bg": isDark
                        ? "rgba(0, 0, 0, 0.16)"
                        : "#edf3fa",
                    "--config-empty-text": isDark ? "#cbd5e1" : "#334155",
                    "--config-button-bg": theme.palette.primary.main,
                    "--config-button-text": theme.palette.primary.contrastText,
                    "--config-button-disabled-bg": isDark
                        ? "#475569"
                        : "#94a3b8",
                    "--config-button-neutral-bg": isDark
                        ? "#334155"
                        : "#edf3fa",
                    "--config-button-neutral-text": theme.palette.text.primary,
                    "--config-button-selected-bg": isDark
                        ? theme.palette.primary.main
                        : "#dbeafe",
                    "--config-button-selected-text": isDark
                        ? theme.palette.primary.contrastText
                        : theme.palette.primary.dark,
                    "--config-modal-backdrop": isDark
                        ? "rgb(2 6 23 / 72%)"
                        : "rgb(226 232 240 / 72%)",
                };
            }}
        >
            <Typography variant="h4" component="h1" gutterBottom>
                SC2 Coop Info v{appVersion}
                {isDev ? " Dev" : ""}
            </Typography>

            <ConfigRoute onThemeModeChange={onThemeModeChange} />
        </Container>
    );
}
