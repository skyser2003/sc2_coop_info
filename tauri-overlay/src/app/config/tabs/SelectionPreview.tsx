import * as React from "react";
import styles from "../page.module.css";

type SelectionPreviewProps = {
    assetUrl: string;
    title: string;
    subtitle?: string;
    kind: "map" | "commander";
    className?: string;
    titleClassName?: string;
    subtitleClassName?: string;
};

export default function SelectionPreview({
    assetUrl,
    title,
    subtitle,
    kind,
    className,
    titleClassName,
    subtitleClassName,
}: SelectionPreviewProps) {
    const [assetFailed, setAssetFailed] = React.useState<boolean>(
        assetUrl === "",
    );

    React.useEffect(() => {
        setAssetFailed(assetUrl === "");
    }, [assetUrl]);

    return (
        <div
            className={[
                styles.selectionPreview,
                kind === "map"
                    ? styles.selectionPreviewMap
                    : styles.selectionPreviewCommander,
                assetFailed ? styles.isFallback : "",
                className,
            ]
                .filter(Boolean)
                .join(" ")}
        >
            {!assetFailed ? (
                <img
                    className={styles.selectionPreviewMedia}
                    src={assetUrl}
                    alt={title}
                    loading="lazy"
                    onError={() => setAssetFailed(true)}
                />
            ) : null}
            <div className={styles.selectionPreviewScrim} />
            {title !== "" ? (
                <div
                    className={[styles.selectionPreviewTitle, titleClassName]
                        .filter(Boolean)
                        .join(" ")}
                >
                    {title}
                </div>
            ) : null}
            {subtitle ? (
                <div
                    className={[
                        styles.selectionPreviewSubtitle,
                        subtitleClassName,
                    ]
                        .filter(Boolean)
                        .join(" ")}
                >
                    {subtitle}
                </div>
            ) : null}
        </div>
    );
}
