/**
 * Foreign key utilities for the admin UI.
 */

import type { TableInfo, ForeignKeyInfo, ColumnInfo, Row, Value, SchemaInfo } from "../types.js";

/**
 * Get the FK info for a column, if it's part of a foreign key.
 */
export function getFkForColumn(table: TableInfo, columnName: string): ForeignKeyInfo | null {
    for (const fk of table.foreign_keys) {
        if (fk.columns.includes(columnName)) {
            return fk;
        }
    }
    return null;
}

/**
 * Check if a column is part of a foreign key.
 */
export function isFkColumn(table: TableInfo, columnName: string): boolean {
    return getFkForColumn(table, columnName) !== null;
}

/**
 * Get the primary key column(s) for a table.
 */
export function getPkColumns(table: TableInfo): ColumnInfo[] {
    return table.columns.filter(c => c.primary_key);
}

/**
 * Get the primary key value from a row.
 */
export function getPkValue(table: TableInfo, row: Row): Value | null {
    const pkCol = table.columns.find(c => c.primary_key);
    if (!pkCol) return null;
    const field = row.fields.find(f => f.name === pkCol.name);
    return field?.value ?? null;
}

/**
 * Determine the best "display" column for a table.
 * Priority: explicit label > name > title > label > first text column > first column
 */
export function getDisplayColumn(table: TableInfo): ColumnInfo | null {
    // First check for explicit label annotation
    const labelCol = table.columns.find(c => c.label);
    if (labelCol) return labelCol;

    const preferredNames = ['name', 'title', 'label', 'display_name', 'username', 'email', 'slug'];

    // Try preferred names
    for (const name of preferredNames) {
        const col = table.columns.find(c => c.name.toLowerCase() === name);
        if (col) return col;
    }

    // Try first text column
    const textCol = table.columns.find(c =>
        c.sql_type.toUpperCase().includes('TEXT') ||
        c.sql_type.toUpperCase().includes('VARCHAR')
    );
    if (textCol) return textCol;

    // Fall back to first non-PK column, or first column
    const nonPk = table.columns.find(c => !c.primary_key);
    return nonPk ?? table.columns[0] ?? null;
}

/**
 * Get the display value for a row (using the display column).
 */
export function getDisplayValue(table: TableInfo, row: Row): string {
    const displayCol = getDisplayColumn(table);
    if (!displayCol) return "(unknown)";

    const field = row.fields.find(f => f.name === displayCol.name);
    if (!field) return "(unknown)";

    return formatValueForDisplay(field.value);
}

/**
 * Format a Value for display.
 */
export function formatValueForDisplay(value: Value): string {
    if (value.tag === "Null") return "(null)";
    if (value.tag === "Bool") return value.value ? "true" : "false";
    if (value.tag === "Bytes") return `<${value.value.length} bytes>`;
    if (value.tag === "String") {
        // Truncate long strings
        if (value.value.length > 50) {
            return value.value.slice(0, 50) + "...";
        }
        return value.value;
    }
    if (typeof value.value === "bigint") {
        return value.value.toString();
    }
    return String(value.value);
}

/**
 * Get a table by name from the schema.
 */
export function getTableByName(schema: SchemaInfo, name: string): TableInfo | null {
    return schema.tables.find(t => t.name === name) ?? null;
}

/**
 * Navigation breadcrumb entry.
 */
export interface BreadcrumbEntry {
    table: string;
    label: string;
    pkValue?: Value;
}

/**
 * Create a breadcrumb label for a table/row.
 */
export function createBreadcrumbLabel(table: TableInfo, row?: Row): string {
    if (!row) return table.name;

    const displayCol = getDisplayColumn(table);
    if (!displayCol) return table.name;

    const field = row.fields.find(f => f.name === displayCol.name);
    if (!field || field.value.tag === "Null") {
        // Fall back to PK value
        const pk = getPkValue(table, row);
        if (pk && pk.tag !== "Null") {
            return `${table.name} #${formatValueForDisplay(pk)}`;
        }
        return table.name;
    }

    const displayValue = formatValueForDisplay(field.value);
    return displayValue.length > 30 ? displayValue.slice(0, 30) + "..." : displayValue;
}
