import { openUrl } from "@tauri-apps/plugin-opener";
import * as React from "react";
import type { JsonObject, JsonValue } from "../types";

type TabGroup = {
    title: string;
    paths?: string[][];
    links?: [string, string][];
    placeholder?: string;
};

type ConfigTab = {
    groups?: TabGroup[];
    title: string;
};

type GenericTabProps = {
    tab: ConfigTab;
    draft: JsonObject | null;
    settings: JsonObject | null;
    onChange: (path: string[], value: JsonValue) => void;
    renderNode: (
        value: JsonValue | undefined,
        templateValue: JsonValue | undefined,
        path: string[],
        depth: number,
        onChange: (path: string[], value: JsonValue) => void,
    ) => React.ReactNode;
    getAtPath: (
        source: JsonObject | null,
        path: string[],
    ) => JsonValue | undefined;
};

export default function GenericTab({
    tab,
    draft,
    settings,
    onChange,
    renderNode,
    getAtPath,
}: GenericTabProps) {
    const groups = tab.groups || [];
    return (
        <div className="tab-content">
            {groups.map((group) => (
                <section key={group.title} className="card group">
                    <h3>{group.title}</h3>
                    {group.placeholder ? (
                        <p className="note">{group.placeholder}</p>
                    ) : null}
                    {group.links ? (
                        <ul className="link-list">
                            {group.links.map(([label, href]) => (
                                <li key={href}>
                                    <a
                                        href={href}
                                        target="_blank"
                                        rel="noreferrer"
                                        onClick={() => openUrl(href)}
                                    >
                                        {label}
                                    </a>
                                </li>
                            ))}
                        </ul>
                    ) : null}
                    {group.paths
                        ? group.paths
                              .map((path) => {
                                  const value = getAtPath(draft, path);
                                  const template = getAtPath(settings, path);
                                  if (
                                      value === undefined &&
                                      template === undefined
                                  ) {
                                      return null;
                                  }
                                  return (
                                      <div
                                          key={path.join(".")}
                                          className="group-field"
                                      >
                                          {renderNode(
                                              value,
                                              template,
                                              path,
                                              0,
                                              onChange,
                                          )}
                                      </div>
                                  );
                              })
                              .filter(Boolean)
                        : null}
                </section>
            ))}
        </div>
    );
}
