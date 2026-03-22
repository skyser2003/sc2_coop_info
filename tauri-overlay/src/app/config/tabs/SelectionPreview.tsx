import * as React from "react";

type SelectionPreviewProps = {
    assetUrl: string;
    title: string;
    subtitle?: string;
    kind: "map" | "commander";
    className?: string;
    titleClassName?: string;
    subtitleClassName?: string;
};

function joinClassNames(...values: Array<string | undefined>): string {
    return values
        .filter((value) => typeof value === "string" && value !== "")
        .join(" ");
}

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
            className={joinClassNames(
                "selection-preview",
                `selection-preview-${kind}`,
                assetFailed ? "is-fallback" : undefined,
                className,
            )}
        >
            {!assetFailed ? (
                <img
                    className="selection-preview-media"
                    src={assetUrl}
                    alt={title}
                    loading="lazy"
                    onError={() => setAssetFailed(true)}
                />
            ) : null}
            <div className="selection-preview-scrim" />
            {title !== "" ? (
                <div
                    className={joinClassNames(
                        "selection-preview-title",
                        titleClassName,
                    )}
                >
                    {title}
                </div>
            ) : null}
            {subtitle ? (
                <div
                    className={joinClassNames(
                        "selection-preview-subtitle",
                        subtitleClassName,
                    )}
                >
                    {subtitle}
                </div>
            ) : null}
        </div>
    );
}
