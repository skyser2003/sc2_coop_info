export type OverlayColorPreviewPayload = {
    color_player1?: string;
    color_player2?: string;
    color_amon?: string;
    color_mastery?: string;
};

export type OverlayLanguagePreviewPayload = {
    language: string;
};

export type OverlayInitColorsDurationPayload = {
    colors: [string | null, string | null, string | null, string | null];
    duration: number;
    show_charts: boolean;
    show_session: boolean;
    session_victory: number;
    session_defeat: number;
    language: string;
};
