import { Fragment, ReactNode } from "react";
import { LanguageManager } from "../../i18n/languageManager";
import type {
    OverlayPlayerInfoPayload,
    OverlayPlayerInfoRow,
} from "../../../bindings/overlay";

function renderPlayerStatRow(
    playerName: string,
    row: OverlayPlayerInfoRow,
): ReactNode {
    if (row.kind === "no_games") {
        return (
            <>
                No games played with{" "}
                <span className="player_stat">{playerName}</span>
                {row.note != null && row.note !== "" ? (
                    <>
                        <br />
                        {row.note}
                    </>
                ) : null}
            </>
        );
    }

    const totalGames = row.wins + row.losses;
    const winRate =
        totalGames > 0 ? Math.round((100 * row.wins) / totalGames) : 0;
    const killRate = Math.round(100 * row.kills);

    return (
        <>
            You played {totalGames} games with{" "}
            <span className="player_stat">{playerName}</span> ({winRate}%
            winrate | {killRate}% kills | {row.apm} APM)
            <br />
            Last game played together: {row.last_seen_relative}
            {row.note != null && row.note !== "" ? (
                <>
                    <br />
                    {row.note}
                </>
            ) : null}
        </>
    );
}

export default function PlayerStatMode({
    payload,
    visible,
    immediate,
    overlayLanguageManager,
}: {
    payload: OverlayPlayerInfoPayload | null;
    visible: boolean;
    immediate: boolean;
    language: string;
    overlayLanguageManager: LanguageManager;
}) {
    const playerRows = payload?.data ?? null;
    const rowEntries = playerRows == null ? [] : Object.entries(playerRows);

    return (
        <div
            id="playerstats"
            style={{
                display: visible ? "block" : "none",
                right: visible ? "2vh" : "-60vh",
                opacity: visible ? 1 : 0,
                transition: immediate ? "all 0s" : undefined,
            }}
        >
            {rowEntries.length > 0
                ? rowEntries.map(([playerName, row]) => (
                      <Fragment key={playerName}>
                          {renderPlayerStatRow(playerName, row)}
                          <br />
                      </Fragment>
                  ))
                : payload != null
                  ? overlayLanguageManager.translate(
                        "ui_overlay_no_player_data",
                    )
                  : null}
        </div>
    );
}
