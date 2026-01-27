<script lang="ts">
    import {
        CaretUp,
        CaretDown,
        Clock,
        Hash,
        TextT,
        ToggleLeft,
        Calendar,
        Timer,
        Binary,
        ArrowSquareOut,
    } from "phosphor-svelte";
    import type {
        Row,
        ColumnInfo,
        Value,
        Sort,
        SortDir,
        TableInfo,
        SchemaInfo,
        SquelClient,
    } from "../types";
    import type { RowExpandConfig } from "../types/config";
    import type { Component } from "svelte";
    import { getFkForColumn, getTableByName } from "../lib/fk-utils";
    import FkCell from "./FkCell.svelte";
    import DynamicIcon from "./DynamicIcon.svelte";
    import MarkdownRenderer from "./MarkdownRenderer.svelte";

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

        onFkClick?: (targetTable: string, pkValue: Value) => void;
        fkLookup?: Map<string, Map<string, Row>>;
        // Time display mode
        timeMode?: "relative" | "absolute";
        // Row expansion
        rowExpand?: RowExpandConfig;
        // Image columns
        imageColumns?: string[];
    }

    let {
        columns,
        rows,
        sort,
        onSort,
        onRowClick,
        table,
        schema,
        client,
        onFkClick,
        fkLookup,
        timeMode = "relative",
        rowExpand,
        imageColumns = [],
    }: Props = $props();

    // Track which rows have expanded content
    let expandedRows = $state<Set<number>>(new Set());

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
        if (
            t === "REAL" ||
            t === "DOUBLE PRECISION" ||
            t.includes("FLOAT") ||
            t.includes("NUMERIC") ||
            t.includes("DECIMAL")
        )
            return Hash;
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
        const pkStr =
            typeof value.value === "bigint" ? value.value.toString() : String(value.value);
        return tableCache.get(pkStr);
    }

    // Get expanded content for a row
    function getExpandedContent(row: Row): string | null {
        if (!rowExpand) return null;
        const field = row.fields.find((f) => f.name === rowExpand.field);
        if (!field || field.value.tag === "Null") return null;
        if (field.value.tag === "String") return field.value.value;
        return String(field.value.value);
    }

    // Check if a column should be rendered as an image
    function isImageColumn(colName: string): boolean {
        return imageColumns.includes(colName);
    }

    // Get preview of content (first N lines)
    function getPreview(
        content: string,
        lines: number = 3,
    ): { preview: string; truncated: boolean } {
        const allLines = content.split("\n");
        if (allLines.length <= lines) {
            return { preview: content, truncated: false };
        }
        return { preview: allLines.slice(0, lines).join("\n"), truncated: true };
    }

    // Toggle expanded state for a row
    function toggleExpanded(rowIndex: number, e: Event) {
        e.stopPropagation();
        const newSet = new Set(expandedRows);
        if (newSet.has(rowIndex)) {
            newSet.delete(rowIndex);
        } else {
            newSet.add(rowIndex);
        }
        expandedRows = newSet;
    }
</script>

<div class="table-container">
    <table class="data-table">
        <thead>
            <tr>
                {#each columns as col}
                    {@const sortDir = getSortDir(col.name)}
                    <th class="table-header" onclick={() => handleHeaderClick(col.name)}>
                        <span class="header-content">
                            {col.name}
                            <span class="sort-icon" class:visible={sortDir !== null}>
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
                {@const expandedContent = getExpandedContent(row)}
                <tr class="table-row" class:clickable onclick={() => handleRowClick(row)}>
                    {#each columns as col}
                        {@const cell = getRowValue(row, col)}
                        {@const TypeIcon = getTypeIcon(col)}
                        {@const fkInfo = getFkInfo(col)}
                        {@const rawValue = getRawValue(row, col)}
                        <td class="table-cell" class:null-value={cell.isNull}>
                            <span class="cell-content">
                                {#if col.icon}
                                    <DynamicIcon name={col.icon} size={12} class="type-icon" />
                                {:else if TypeIcon}
                                    <TypeIcon size={12} class="type-icon" />
                                {/if}
                                {#if isImageColumn(col.name) && rawValue.tag === "String" && rawValue.value}
                                    <img src={rawValue.value} alt={col.name} class="cell-image" />
                                {:else if fkInfo && client && rawValue.tag !== "Null"}
                                    {@const cachedRow = getCachedFkRow(
                                        fkInfo.fkTable.name,
                                        rawValue,
                                    )}
                                    <FkCell
                                        value={rawValue}
                                        fkTable={fkInfo.fkTable}
                                        fkColumn={fkInfo.fkColumn}
                                        {client}
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
                {#if expandedContent}
                    {@const rowIndex = rows.indexOf(row)}
                    {@const isExpanded = expandedRows.has(rowIndex)}
                    {@const previewLines = rowExpand?.previewLines ?? 3}
                    {@const previewData = getPreview(expandedContent, previewLines)}
                    {@const displayContent = isExpanded ? expandedContent : previewData.preview}
                    <tr class="expanded-row">
                        <td colspan={columns.length} class="expanded-cell">
                            {#if rowExpand?.render === "markdown"}
                                <div class="markdown-content">
                                    <MarkdownRenderer content={displayContent} />
                                </div>
                            {:else if rowExpand?.render === "code"}
                                <pre class="code-content"><code>{displayContent}</code></pre>
                            {:else}
                                <div class="text-content">{displayContent}</div>
                            {/if}
                            {#if previewData.truncated}
                                <button
                                    type="button"
                                    class="expand-toggle"
                                    onclick={(e) => toggleExpanded(rowIndex, e)}
                                >
                                    {isExpanded ? "Show less" : "Show more..."}
                                </button>
                            {/if}
                        </td>
                    </tr>
                {/if}
            {/each}
        </tbody>
    </table>
</div>

<style>
    .table-container {
        flex: 1;
        overflow: auto;
    }

    .data-table {
        width: 100%;
        border-collapse: collapse;
        font-size: 0.875rem;
    }

    .table-header {
        padding: 0.75rem 1rem;
        text-align: left;
        color: var(--muted-foreground);
        font-weight: 500;
        position: sticky;
        top: 0;
        cursor: pointer;
        user-select: none;
        white-space: nowrap;
        background-color: var(--background);
        transition: color 0.15s;
    }

    .table-header:hover {
        color: var(--foreground);
    }

    .header-content {
        display: inline-flex;
        align-items: center;
        gap: 0.5rem;
    }

    .sort-icon {
        opacity: 0;
        transition: opacity 0.15s;
    }

    .sort-icon.visible {
        opacity: 1;
    }

    .table-row {
        border-top: 1px solid var(--border);
        transition: all 0.15s;
    }

    .table-row.clickable {
        cursor: pointer;
        border-left: 2px solid transparent;
    }

    .table-row.clickable:hover {
        background-color: color-mix(in oklch, var(--accent) 50%, transparent);
        border-left-color: var(--primary);
    }

    .table-cell {
        padding: 0.75rem 1rem;
        font-size: 0.875rem;
        max-width: 300px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .table-cell.null-value {
        color: var(--muted-foreground);
    }

    .cell-content {
        display: inline-flex;
        align-items: center;
        gap: 0.375rem;
    }

    .cell-content :global(.type-icon) {
        color: color-mix(in oklch, var(--muted-foreground) 60%, transparent);
        flex-shrink: 0;
    }

    .cell-image {
        width: 2rem;
        height: 2rem;
        border-radius: 9999px;
        object-fit: cover;
    }

    .expanded-row {
        background-color: color-mix(in oklch, var(--muted) 30%, transparent);
    }

    .expanded-cell {
        padding: 0.75rem 1rem;
        font-size: 0.875rem;
    }

    .markdown-content {
        max-width: none;
    }

    .code-content {
        font-family: ui-monospace, monospace;
        font-size: 0.75rem;
        background-color: var(--muted);
        padding: 0.75rem;
        border-radius: var(--radius-md);
        overflow-x: auto;
        margin: 0;
    }

    .text-content {
        white-space: pre-wrap;
    }

    .expand-toggle {
        margin-top: 0.5rem;
        font-size: 0.75rem;
        color: var(--muted-foreground);
        background: none;
        border: none;
        padding: 0;
        cursor: pointer;
        transition: color 0.15s;
    }

    .expand-toggle:hover {
        color: var(--foreground);
    }
</style>
