import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

class GoogleAnalyticsHtmlPlugin {
    private static readonly blockPattern =
        /\r?\n\s*<!-- Google tag \(gtag\.js\) -->\s*<script\s+async\s+src="https:\/\/www\.googletagmanager\.com\/gtag\/js\?id=G-K12WZBGJF7"\s*><\/script>\s*<script\s+nonce="c2NvT3ZlcmxheUdh">[\s\S]*?gtag\("config", "G-K12WZBGJF7"\);\s*<\/script>/;

    public static create(stripGoogleAnalytics: boolean): Plugin {
        return {
            name: "sco-google-analytics-html",
            transformIndexHtml(html: string): string {
                if (!stripGoogleAnalytics) {
                    return html;
                }

                return GoogleAnalyticsHtmlPlugin.removeGoogleAnalytics(html);
            },
        };
    }

    private static removeGoogleAnalytics(html: string): string {
        return html.replace(GoogleAnalyticsHtmlPlugin.blockPattern, "");
    }
}

// https://vite.dev/config/
export default defineConfig(({ command }) => ({
    plugins: [react(), GoogleAnalyticsHtmlPlugin.create(command === "serve")],

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent Vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
        port: 5173,
        strictPort: true,
        host: host || false,
        hmr: host
            ? {
                  protocol: "ws",
                  host,
                  port: 1421,
              }
            : undefined,
        watch: {
            // 3. tell Vite to ignore watching `src-tauri`
            ignored: ["**/src-tauri/**"],
        },
    },
}));
