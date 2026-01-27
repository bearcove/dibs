import type { Component } from "svelte";
import type { Filter } from "../types";

// =============================================================================
// Sort and Filter Configuration
// =============================================================================

export interface SortConfig {
    field: string;
    direction: "asc" | "desc";
}

export interface FilterConfig {
    field: string;
    op: "eq" | "ne" | "lt" | "lte" | "gt" | "gte" | "like" | "ilike" | "null" | "notnull";
    value?: unknown;
}

// =============================================================================
// Dashboard Configuration
// =============================================================================

export interface DashboardConfig {
    /** Dashboard title */
    title?: string;

    /** Tiles to display */
    tiles: DashboardTile[];
}

export type DashboardTile =
    | LatestRecordsTile
    | CountTile
    | QuickLinksTile
    | CustomTile;

export interface LatestRecordsTile {
    type: "latest";
    table: string;
    title?: string;
    limit?: number;           // default: 5
    columns?: string[];       // which columns to show
    sort?: SortConfig;        // default: by created_at or PK desc
}

export interface CountTile {
    type: "count";
    table: string;
    title?: string;
    filter?: FilterConfig[];  // optional filter for counting
    icon?: string;
}

export interface QuickLinksTile {
    type: "links";
    title?: string;
    links: { label: string; table: string; filter?: FilterConfig[] }[];
}

export interface CustomTile {
    type: "custom";
    component: Component;     // Svelte component
}

// =============================================================================
// Table Configuration
// =============================================================================

export interface TableConfig {
    /** Hide this table from the sidebar */
    hidden?: boolean;

    /** Custom display name */
    label?: string;

    /** List view configuration */
    list?: ListViewConfig;

    /** Detail page configuration (unified view/edit) */
    detail?: DetailConfig;

    /** Relations to show in detail view */
    relations?: RelationConfig[];
}

export interface ListViewConfig {
    /** Columns to display (in order). If not set, shows all. */
    columns?: string[];

    /** Default sort */
    defaultSort?: SortConfig;

    /** Default filters (applied on load) */
    defaultFilters?: FilterConfig[];

    /** Columns that can be sorted (if not set, all sortable) */
    sortableColumns?: string[];

    /** Columns that can be filtered (if not set, all filterable) */
    filterableColumns?: string[];

    /** Max rows per page */
    pageSize?: number;  // default: 25

    /** Expanded content shown below each row */
    rowExpand?: RowExpandConfig;

    /** Columns to render as images (e.g., avatar_url) */
    imageColumns?: string[];
}

export interface RowExpandConfig {
    /** Column name containing the content to expand */
    field: string;

    /** How to render the content */
    render?: "text" | "markdown" | "code";

    /** For code rendering, the language */
    lang?: string;

    /** Lines to show before truncating (default: 3) */
    previewLines?: number;
}

export interface DetailConfig {
    /** Fields to show (in order). If not set, shows all non-hidden. */
    fields?: (string | FieldGroupConfig)[];

    /** Fields that are read-only (displayed but not click-to-edit) */
    readOnly?: string[];

    /** Fields hidden from detail view entirely */
    hidden?: string[];

    /** Show related tables section */
    showRelations?: boolean;  // default: true
}

export interface FieldGroupConfig {
    title: string;
    fields: string[];
    collapsed?: boolean;
}

// =============================================================================
// Relation Configuration
// =============================================================================

export interface RelationConfig {
    /** Target table name */
    table: string;

    /** FK column in target table that references us */
    via: string;

    /** Custom label for this relation */
    label?: string;

    /** Columns to show in relation list */
    columns?: string[];

    /** Default expanded state */
    expanded?: boolean;

    /** Max records to load */
    limit?: number;  // default: 10
}

// =============================================================================
// Field Renderer (advanced)
// =============================================================================

export interface FieldRenderer {
    /** Display mode component */
    display: Component;
    /** Edit mode component */
    edit: Component;
}

// =============================================================================
// Table Defaults
// =============================================================================

export interface TableDefaults {
    /** Default page size for list views */
    pageSize?: number;

    /** Default relation limit */
    relationLimit?: number;
}

// =============================================================================
// Top-Level Configuration
// =============================================================================

export interface DibsAdminConfig {
    /** Dashboard configuration (optional) */
    dashboard?: DashboardConfig;

    /** Per-table configurations */
    tables?: Record<string, TableConfig>;

    /** Global defaults for all tables */
    defaults?: TableDefaults;

    /** Custom field renderers (advanced) */
    fieldRenderers?: Record<string, FieldRenderer>;
}
