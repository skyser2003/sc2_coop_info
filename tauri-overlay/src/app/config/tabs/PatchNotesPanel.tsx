import { openUrl } from "@tauri-apps/plugin-opener";
import * as React from "react";
import type { LanguageManager } from "../../i18n/languageManager";
import styles from "../configStyles";
import {
    GITHUB_RELEASES_PAGE_URL,
    GitHubReleasesClient,
    type GitHubRelease,
} from "../githubReleases";

type LoadStatus = "loading" | "ready" | "error";

type PatchNotesPanelProps = {
    appVersion: string;
    languageManager: LanguageManager;
    reloadNumber: number;
};

export default function PatchNotesPanel({
    appVersion,
    languageManager,
    reloadNumber,
}: PatchNotesPanelProps): React.ReactNode {
    const [loadStatus, setLoadStatus] = React.useState<LoadStatus>("loading");
    const [releases, setReleases] = React.useState<readonly GitHubRelease[]>(
        [],
    );
    const [requestNumber, setRequestNumber] = React.useState<number>(0);
    const releasesClient = React.useMemo(() => new GitHubReleasesClient(), []);
    const normalizedAppVersion = React.useMemo(
        () => appVersion.trim().replace(/^v/i, ""),
        [appVersion],
    );
    const t = (id: string): string => languageManager.translate(id);

    React.useEffect(() => {
        const abortController = new AbortController();
        setLoadStatus("loading");

        void releasesClient
            .latest(abortController.signal)
            .then((latestReleases) => {
                setReleases(latestReleases);
                setLoadStatus("ready");
            })
            .catch(() => {
                if (!abortController.signal.aborted) {
                    setLoadStatus("error");
                }
            });

        return () => abortController.abort();
    }, [releasesClient, reloadNumber, requestNumber]);

    const openExternalUrl = React.useCallback((url: string): void => {
        void openUrl(url).catch(() => undefined);
    }, []);
    const followExternalLink = React.useCallback(
        (event: React.MouseEvent<HTMLAnchorElement>, url: string): void => {
            event.preventDefault();
            openExternalUrl(url);
        },
        [openExternalUrl],
    );

    return (
        <section
            className={styles.patchNotesPanel}
            aria-labelledby="settings-patch-notes-title"
        >
            <h3
                id="settings-patch-notes-title"
                className={styles.mainSettingsGroupTitle}
            >
                <a
                    className={styles.patchNotesTitleLink}
                    href={GITHUB_RELEASES_PAGE_URL}
                    target="_blank"
                    rel="noreferrer"
                    onClick={(event) =>
                        followExternalLink(event, GITHUB_RELEASES_PAGE_URL)
                    }
                >
                    {t("ui_patch_notes_title")}
                </a>
            </h3>
            <div
                className={styles.patchNotesScroll}
                data-testid="patch-notes-scroll"
            >
                {loadStatus === "loading" ? (
                    <p className={styles.patchNotesStatus} role="status">
                        {t("ui_patch_notes_loading")}
                    </p>
                ) : null}
                {loadStatus === "error" ? (
                    <div className={styles.patchNotesStatus} role="alert">
                        <p>{t("ui_patch_notes_error")}</p>
                        <button
                            type="button"
                            className={styles.buttonNormal}
                            onClick={() =>
                                setRequestNumber((current) => current + 1)
                            }
                        >
                            {t("ui_patch_notes_retry")}
                        </button>
                    </div>
                ) : null}
                {loadStatus === "ready" && releases.length === 0 ? (
                    <p className={styles.patchNotesStatus}>
                        {t("ui_patch_notes_empty")}
                    </p>
                ) : null}
                {releases.map((release, releaseIndex) => (
                    <article
                        key={release.tagName}
                        className={styles.patchNoteCard}
                        data-testid="patch-note"
                    >
                        <h4>
                            <a
                                className={styles.patchNoteVersionLink}
                                href={release.releaseUrl}
                                target="_blank"
                                rel="noreferrer"
                                onClick={(event) =>
                                    followExternalLink(
                                        event,
                                        release.releaseUrl,
                                    )
                                }
                            >
                                {release.tagName}
                            </a>
                            <span className={styles.patchNoteLabels}>
                                {release.tagName.trim().replace(/^v/i, "") ===
                                normalizedAppVersion ? (
                                    <span
                                        className={[
                                            styles.patchNoteLabel,
                                            styles.patchNoteLabelCurrent,
                                        ]
                                            .filter(Boolean)
                                            .join(" ")}
                                        data-testid="patch-note-label"
                                    >
                                        {t("ui_patch_notes_current")}
                                    </span>
                                ) : null}
                                {releaseIndex === 0 ? (
                                    <span
                                        className={[
                                            styles.patchNoteLabel,
                                            styles.patchNoteLabelLatest,
                                        ]
                                            .filter(Boolean)
                                            .join(" ")}
                                        data-testid="patch-note-label"
                                    >
                                        {t("ui_patch_notes_latest")}
                                    </span>
                                ) : null}
                            </span>
                        </h4>
                        <p className={styles.patchNoteBody}>
                            {release.body || t("ui_patch_notes_no_content")}
                        </p>
                    </article>
                ))}
                <div className={styles.patchNotesMore}>
                    <button
                        type="button"
                        className={styles.buttonNormal}
                        onClick={() =>
                            openExternalUrl(GITHUB_RELEASES_PAGE_URL)
                        }
                    >
                        {t("ui_patch_notes_show_more")}
                    </button>
                </div>
            </div>
        </section>
    );
}
