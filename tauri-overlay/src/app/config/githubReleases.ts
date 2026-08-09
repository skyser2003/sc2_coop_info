import type { JsonObject, JsonValue } from "./types";

export const GITHUB_RELEASES_PAGE_URL =
    "https://github.com/skyser2003/sc2_coop_info/releases";

const GITHUB_RELEASES_API_URL =
    "https://api.github.com/repos/skyser2003/sc2_coop_info/releases?per_page=10";
const GITHUB_API_VERSION = "2026-03-10";
const RELEASE_LIMIT = 10;

export type GitHubRelease = {
    readonly tagName: string;
    readonly body: string;
    readonly releaseUrl: string;
};

export class GitHubReleasesClient {
    private readonly endpoint: string;

    public constructor(endpoint: string = GITHUB_RELEASES_API_URL) {
        this.endpoint = endpoint;
    }

    public async latest(
        abortSignal: AbortSignal,
    ): Promise<readonly GitHubRelease[]> {
        const response = await fetch(this.endpoint, {
            headers: {
                Accept: "application/vnd.github+json",
                "X-GitHub-Api-Version": GITHUB_API_VERSION,
            },
            signal: abortSignal,
        });
        if (!response.ok) {
            throw new Error(
                `GitHub releases request failed with status ${response.status}`,
            );
        }

        const payload = JSON.parse(await response.text()) as JsonValue;
        if (!Array.isArray(payload)) {
            throw new Error("GitHub releases response was not a list");
        }

        const releases: GitHubRelease[] = [];
        for (const entry of payload) {
            const release = this.parseRelease(entry);
            if (release !== null) {
                releases.push(release);
            }
            if (releases.length === RELEASE_LIMIT) {
                break;
            }
        }
        return releases;
    }

    private parseRelease(value: JsonValue): GitHubRelease | null {
        if (!this.isJsonObject(value)) {
            return null;
        }
        if (value.draft === true) {
            return null;
        }

        const tagName =
            typeof value.tag_name === "string" ? value.tag_name.trim() : "";
        if (tagName === "") {
            return null;
        }

        return {
            tagName,
            body: typeof value.body === "string" ? value.body.trim() : "",
            releaseUrl: this.releasePageUrl(value, tagName),
        };
    }

    private releasePageUrl(value: JsonObject, tagName: string): string {
        const apiReleaseUrl =
            typeof value.html_url === "string" ? value.html_url.trim() : "";
        if (apiReleaseUrl.startsWith(`${GITHUB_RELEASES_PAGE_URL}/tag/`)) {
            return apiReleaseUrl;
        }
        return `${GITHUB_RELEASES_PAGE_URL}/tag/${encodeURIComponent(tagName)}`;
    }

    private isJsonObject(value: JsonValue): value is JsonObject {
        return (
            value !== null && typeof value === "object" && !Array.isArray(value)
        );
    }
}
