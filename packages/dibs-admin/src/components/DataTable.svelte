<script lang="ts">
    import { CaretUp, CaretDown, Clock, Hash, TextT, ToggleLeft, Calendar, Timer, Binary, ArrowSquareOut } from "phosphor-svelte";
    import type { Row, ColumnInfo, Value, Sort, SortDir, TableInfo, SchemaInfo, SquelClient } from "../types.js";
    import type { Component } from "svelte";
    import { getFkForColumn, getTableByName } from "../lib/fk-utils.js";
    import FkCell from "./FkCell.svelte";
    import DynamicIcon from "./DynamicIcon.svelte";

    interface Props {
        columns: ColumnInfo[];
        rows: Row[];
        sort: Sort | null;
        onSort: (column: string) => void;
        onRowClick?: (row: Row) => void;
        // FK support
        table?: TableInfo;
        schema?: SchemaInfo;
        client?: SquelClient;
        databaseUrl?: string;
        onFkClick?: (targetTable: string, pkValue: Value) => void;
        fkLookup?: Map<string, Map<string, Row>>;
    }

    let { columns, rows, sort, onSort, onRowClick, table, schema, client, databaseUrl, onFkClick, fkLookup }: Props = $props();

    // Time display mode: "relative" or "absolute"
    let timeMode = $state<"relative" | "absolute">("relative");

    function isTimestampColumn(col: ColumnInfo): boolean {
        const t = col.sql_type.toUpperCase();
        return t.includes("TIMESTAMP") || t.includes("TIMESTAMPTZ");
    }

    type IconComponent = Component<{ size?: number; class?: string }>;

    function getTypeIcon(col: ColumnInfo): IconComponent | null {
        const t = col.sql_type.toUpperCase();
        if (t.includes("TIMESTAMP") || t.includes("TIMESTAMPTZ")) return Clock;
        if (t === "DATE") return Calendar;
        if (t === "TIME") return Timer;
        if (t.includes("INT") || t === "BIGINT" || t === "SMALLINT" || t === "INTEGER") return Hash;
        if (t === "REAL" || t === "DOUBLE PRECISION" || t.includes("FLOAT") || t.includes("NUMERIC") || t.includes("DECIMAL")) return Hash;
        if (t === "BOOLEAN" || t === "BOOL") return ToggleLeft;
        if (t === "TEXT" || t.includes("VARCHAR") || t.includes("CHAR")) return TextT;
        if (t === "BYTEA") return Binary;
        return null;
    }

    function parseTimestamp(value: string): Date | null {
        // Remove quotes if present
        const cleaned = value.replace(/^"|"$/g, "");
        const date = new Date(cleaned);
        return isNaN(date.getTime()) ? null : date;
    }

    function formatRelativeTime(date: Date): string {
        const now = new Date();
        const diffMs = now.getTime() - date.getTime();
        const diffSec = Math.floor(diffMs / 1000);
        const diffMin = Math.floor(diffSec / 60);
        const diffHour = Math.floor(diffMin / 60);
        const diffDay = Math.floor(diffHour / 24);

        if (diffSec < 60) return "just now";
        if (diffMin < 60) return `${diffMin}m ago`;
        if (diffHour < 24) return `${diffHour}h ago`;
        if (diffDay < 30) return `${diffDay}d ago`;
        return date.toLocaleDateString();
    }

    function formatValue(value: Value, col?: ColumnInfo): string {
        if (value.tag === "Null") return "null";
        if (typeof value.value === "bigint") {
            return value.value.toString();
        }
        if (value.tag === "Bytes") {
            return `<${value.value.length} bytes>`;
        }
        if (value.tag === "String") {
            // Check if this is a timestamp column and format accordingly
            if (col && isTimestampColumn(col) && timeMode === "relative") {
                const date = parseTimestamp(value.value);
                if (date) return formatRelativeTime(date);
            }
            if (value.value.length > 100) {
                return value.value.slice(0, 100) + "...";
            }
            return value.value;
        }
        return JSON.stringify(value.value);
    }

    function getRowValue(row: Row, col: ColumnInfo): { value: string; isNull: boolean } {
        const field = row.fields.find((f) => f.name === col.name);
        if (!field) return { value: "null", isNull: true };
        return {
            value: formatValue(field.value, col),
            isNull: field.value.tag === "Null",
        };
    }

    function handleHeaderClick(colName: string) {
        onSort(colName);
    }

    function handleRowClick(row: Row) {
        onRowClick?.(row);
    }

    function getSortDir(colName: string): SortDir["tag"] | null {
        if (sort && sort.field === colName) {
            return sort.dir.tag;
        }
        return null;
    }

    // FK helpers
    function getFkInfo(col: ColumnInfo): { fkTable: TableInfo; fkColumn: string } | null {
        if (!table || !schema) return null;
        const fk = getFkForColumn(table, col.name);
        if (!fk) return null;
        const targetTable = getTableByName(schema, fk.references_table);
        if (!targetTable) return null;
        // Get the corresponding FK column (same index as our column in the FK)
        const colIndex = fk.columns.indexOf(col.name);
        const fkColumn = fk.references_columns[colIndex] ?? fk.references_columns[0];
        return { fkTable: targetTable, fkColumn };
    }

    function handleFkClick(targetTable: string, value: Value) {
        if (value.tag !== "Null") {
            onFkClick?.(targetTable, value);
        }
    }

    function getRawValue(row: Row, col: ColumnInfo): Value {
        const field = row.fields.find((f) => f.name === col.name);
        return field?.value ?? { tag: "Null" };
    }

    // Get a cached FK row from the lookup
    function getCachedFkRow(tableName: string, value: Value): Row | undefined {
        if (!fkLookup || value.tag === "Null") return undefined;
        const tableCache = fkLookup.get(tableName);
        if (!tableCache) return undefined;
        const pkStr = typeof value.value === "bigint" ? value.value.toString() : String(value.value);
        return tableCache.get(pkStr);
    }
</script>

<div class="flex-1 overflow-auto">
    {#if columns.some(isTimestampColumn)}
        <div class="flex justify-end mb-3">
            <button
                class="inline-flex items-center gap-2 text-xs text-muted-foreground hover:text-foreground transition-colors"
                onclick={() => timeMode = timeMode === "relative" ? "absolute" : "relative"}
            >
                <Clock size={14} />
                {timeMode === "relative" ? "relative" : "absolute"}
            </button>
        </div>
    {/if}
    <table class="w-full border-collapse text-sm">
        <thead>
            <tr>
                {#each columns as col}
                    {@const sortDir = getSortDir(col.name)}
                    <th
                        class="px-4 py-3 text-left text-muted-foreground font-medium sticky top-0 cursor-pointer select-none whitespace-nowrap bg-background hover:text-foreground transition-colors"
                        onclick={() => handleHeaderClick(col.name)}
                    >
                        <span class="inline-flex items-center gap-2">
                            {col.name}
                            <span
                                class="opacity-0 transition-opacity"
                                class:opacity-100={sortDir !== null}
                            >
                                {#if sortDir === "Desc"}
                                    <CaretDown size={12} weight="bold" />
                                {:else}
                                    <CaretUp size={12} weight="bold" />
                                {/if}
                            </span>
                        </span>
                    </th>
                {/each}
            </tr>
        </thead>
        <tbody>
            {#each rows as row}
                {@const clickable = onRowClick !== undefined}
                <tr
                    class="border-t border-border transition-all duration-150 {clickable
                        ? 'cursor-pointer border-l-2 border-l-transparent hover:bg-accent/50 hover:border-l-primary'
                        : ''}"
                    onclick={() => handleRowClick(row)}
                >
                    {#each columns as col}
                        {@const cell = getRowValue(row, col)}
                        {@const TypeIcon = getTypeIcon(col)}
                        {@const fkInfo = getFkInfo(col)}
                        {@const rawValue = getRawValue(row, col)}
                        <td
                            class="px-4 py-3 text-sm max-w-[300px] overflow-hidden text-ellipsis whitespace-nowrap"
                            class:text-muted-foreground={cell.isNull}
                        >
                            <span class="inline-flex items-center gap-1.5">
                                {#if col.icon}
                                    <DynamicIcon name={col.icon} size={12} class="text-muted-foreground/60 flex-shrink-0" />
                                {:else if TypeIcon}
                                    <TypeIcon size={12} class="text-muted-foreground/60 flex-shrink-0" />
                                {/if}
                                {#if fkInfo && client && databaseUrl && rawValue.tag !== "Null"}
                                    {@const cachedRow = getCachedFkRow(fkInfo.fkTable.name, rawValue)}
                                    <FkCell
                                        value={rawValue}
                                        fkTable={fkInfo.fkTable}
                                        fkColumn={fkInfo.fkColumn}
                                        {client}
                                        {databaseUrl}
                                        onClick={() => handleFkClick(fkInfo.fkTable.name, rawValue)}
                                        {cachedRow}
                                    />
                                {:else}
                                    {cell.value}
                                {/if}
                            </span>
                        </td>
                    {/each}
                </tr>
            {/each}
        </tbody>
    </table>
</div>
