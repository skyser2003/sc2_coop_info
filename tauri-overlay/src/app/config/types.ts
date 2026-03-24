export type DifficultyFilterKey =
    | "Casual"
    | "Normal"
    | "Hard"
    | "Brutal"
    | "BrutalPlus1"
    | "BrutalPlus2"
    | "BrutalPlus3"
    | "BrutalPlus4"
    | "BrutalPlus5"
    | "BrutalPlus6";

export type DifficultyFilters = Record<DifficultyFilterKey, boolean>;
