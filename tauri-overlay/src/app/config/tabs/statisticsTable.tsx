import type * as React from "react";
import { sortIndicator, type SortState } from "./tableSort";
import styles from "../page.module.css";

export type HeaderColumn = {
    key: string;
    label: string;
    className?: string;
};

export function tableHeader(
    columns: readonly HeaderColumn[],
    sortState: SortState = null,
    onSort: ((key: string) => void) | null = null,
): React.ReactNode {
    return (
        <thead>
            <tr>
                {columns.map((column) => (
                    <th key={column.key} className={column.className}>
                        {onSort ? (
                            <button
                                type="button"
                                className={styles.tableSortBtn}
                                onClick={() => onSort(column.key)}
                            >
                                {`${column.label}${sortIndicator(sortState, column.key)}`}
                            </button>
                        ) : (
                            column.label
                        )}
                    </th>
                ))}
            </tr>
        </thead>
    );
}
