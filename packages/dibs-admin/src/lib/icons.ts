/**
 * Icon resolution utilities for columns and tables.
 *
 * This module centralizes all icon logic:
 * - Custom icons (col.icon) use DynamicIcon (Lucide) via string names
 * - Type-based icons use Phosphor components
 * - FK columns inherit from their target table
 * - Tables have icons or fall back to "table"
 */

import type { Component } from "svelte";
import type { ColumnInfo, TableInfo, SchemaInfo } from "../types";
import {
  ClockIcon,
  HashIcon,
  TextTIcon,
  ToggleLeftIcon,
  CalendarIcon,
  TimerIcon,
  BinaryIcon,
  FileTextIcon,
  BracketsSquareIcon,
  CodeIcon,
  TableIcon,
  LinkIcon,
} from "phosphor-svelte";
import { getFkForColumn, getTableByName } from "./fk-utils";

export type IconComponent = Component<{ size?: number; class?: string }>;

/** Default icon for tables */
export const DEFAULT_TABLE_ICON = "table";

/** Default Phosphor icon component for fields */
export const DefaultFieldIcon = HashIcon;

/** Default Phosphor icon component for tables */
export const DefaultTableIcon = TableIcon;

/** Icon for FK fields when target table has no icon */
export const FkFieldIcon = LinkIcon;

/**
 * Get the appropriate Phosphor icon component based on SQL type.
 */
export function getTypeIcon(sqlType: string): IconComponent {
  const t = sqlType.toUpperCase();
  if (t.includes("TIMESTAMP") || t.includes("TIMESTAMPTZ")) return ClockIcon;
  if (t === "DATE") return CalendarIcon;
  if (t === "TIME") return TimerIcon;
  if (t.includes("INT") || t === "BIGINT" || t === "SMALLINT" || t === "INTEGER") return HashIcon;
  if (
    t === "REAL" ||
    t === "DOUBLE PRECISION" ||
    t.includes("FLOAT") ||
    t.includes("NUMERIC") ||
    t.includes("DECIMAL")
  ) {
    return HashIcon;
  }
  if (t === "BOOLEAN" || t === "BOOL") return ToggleLeftIcon;
  if (t.includes("JSON")) return BracketsSquareIcon;
  if (t === "TEXT" || t.includes("VARCHAR") || t.includes("CHAR")) return TextTIcon;
  if (t === "BYTEA") return BinaryIcon;
  return HashIcon;
}

/**
 * Get the appropriate Phosphor icon component for a language (used for code fields).
 */
export function getLangIcon(lang: string | null | undefined): IconComponent | null {
  if (!lang) return null;
  switch (lang.toLowerCase()) {
    case "markdown":
    case "md":
      return FileTextIcon;
    case "json":
      return BracketsSquareIcon;
    case "html":
    case "css":
    case "javascript":
    case "js":
    case "typescript":
    case "ts":
      return CodeIcon;
    default:
      return CodeIcon;
  }
}

/**
 * Result of resolving a field's icon.
 * Either a custom string name (for DynamicIcon/Lucide) or a Phosphor component.
 */
export type ResolvedIcon =
  | { type: "custom"; name: string }
  | { type: "component"; Icon: IconComponent };

/**
 * Resolve the icon for a column, considering:
 * 1. Custom icon string (col.icon) - highest priority
 * 2. Language icon for code fields (col.lang)
 * 3. FK target table icon
 * 4. SQL type-based icon - fallback
 */
export function resolveFieldIcon(
  col: ColumnInfo,
  table?: TableInfo,
  schema?: SchemaInfo,
): ResolvedIcon {
  // 1. Custom icon takes priority
  if (col.icon) {
    return { type: "custom", name: col.icon };
  }

  // 2. Language-based icon for code fields
  const langIcon = getLangIcon(col.lang);
  if (langIcon) {
    return { type: "component", Icon: langIcon };
  }

  // 3. FK columns inherit from target table
  if (table && schema) {
    const fk = getFkForColumn(table, col.name);
    if (fk) {
      const targetTable = getTableByName(schema, fk.references_table);
      if (targetTable) {
        if (targetTable.icon) {
          // Target table has a custom icon string
          return { type: "custom", name: targetTable.icon };
        } else {
          // Target table has no icon, use FK link icon
          return { type: "component", Icon: FkFieldIcon };
        }
      }
    }
  }

  // 4. Fall back to SQL type-based icon
  return { type: "component", Icon: getTypeIcon(col.sql_type) };
}

/**
 * Resolve the icon for a table.
 * Returns the custom icon name if set, otherwise the default table icon name.
 */
export function resolveTableIcon(table: TableInfo): string {
  return table.icon ?? DEFAULT_TABLE_ICON;
}
