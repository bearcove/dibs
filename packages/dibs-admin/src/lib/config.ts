import type {
    DibsAdminConfig,
    TableConfig,
    ListViewConfig,
    DetailConfig,
    RelationConfig,
    SortConfig,
    FilterConfig,
    TableDefaults,
    RowExpandConfig,
} from "../types/config";
import type { TableInfo, ColumnInfo, Filter, Sort, Value } from "../types";

// =============================================================================
// Default Values
// =============================================================================

const DEFAULT_PAGE_SIZE = 25;
const DEFAULT_RELATION_LIMIT = 10;

const DEFAULT_TABLE_DEFAULTS: Required<TableDefaults> = {
    pageSize: DEFAULT_PAGE_SIZE,
    relationLimit: DEFAULT_RELATION_LIMIT,
};

// =============================================================================
// Config Resolution Helpers
// =============================================================================

/**
 * Get resolved defaults (merging user defaults with built-in defaults)
 */
export function getDefaults(config?: DibsAdminConfig): Required<TableDefaults> {
    if (!config?.defaults) return DEFAULT_TABLE_DEFAULTS;
    return {
        ...DEFAULT_TABLE_DEFAULTS,
        ...config.defaults,
    };
}

/**
 * Get config for a specific table, or undefined if not configured
 */
export function getTableConfig(
    config: DibsAdminConfig | undefined,
    tableName: string
): TableConfig | undefined {
    return config?.tables?.[tableName];
}

/**
 * Check if a table should be hidden from the sidebar
 */
export function isTableHidden(
    config: DibsAdminConfig | undefined,
    tableName: string
): boolean {
    return getTableConfig(config, tableName)?.hidden === true;
}

/**
 * Get display label for a table
 */
export function getTableLabel(
    config: DibsAdminConfig | undefined,
    tableName: string
): string {
    return getTableConfig(config, tableName)?.label ?? tableName;
}

/**
 * Get list view config for a table
 */
export function getListConfig(
    config: DibsAdminConfig | undefined,
    tableName: string
): ListViewConfig | undefined {
    return getTableConfig(config, tableName)?.list;
}

/**
 * Get detail view config for a table
 */
export function getDetailConfig(
    config: DibsAdminConfig | undefined,
    tableName: string
): DetailConfig | undefined {
    return getTableConfig(config, tableName)?.detail;
}

/**
 * Get relations config for a table
 */
export function getRelationsConfig(
    config: DibsAdminConfig | undefined,
    tableName: string
): RelationConfig[] | undefined {
    return getTableConfig(config, tableName)?.relations;
}

// =============================================================================
// List View Helpers
// =============================================================================

/**
 * Filter and order columns based on list config
 */
export function getDisplayColumns(
    columns: ColumnInfo[],
    listConfig: ListViewConfig | undefined
): ColumnInfo[] {
    if (!listConfig?.columns) {
        return columns;
    }

    // Return columns in the specified order, filtering out any that don't exist
    const columnMap = new Map(columns.map((c) => [c.name, c]));
    const result: ColumnInfo[] = [];

    for (const name of listConfig.columns) {
        const col = columnMap.get(name);
        if (col) {
            result.push(col);
        }
    }

    return result;
}

/**
 * Check if a column is sortable
 */
export function isColumnSortable(
    colName: string,
    listConfig: ListViewConfig | undefined
): boolean {
    if (!listConfig?.sortableColumns) {
        return true; // All sortable by default
    }
    return listConfig.sortableColumns.includes(colName);
}

/**
 * Check if a column is filterable
 */
export function isColumnFilterable(
    colName: string,
    listConfig: ListViewConfig | undefined
): boolean {
    if (!listConfig?.filterableColumns) {
        return true; // All filterable by default
    }
    return listConfig.filterableColumns.includes(colName);
}

/**
 * Get page size from config or default
 */
export function getPageSize(
    config: DibsAdminConfig | undefined,
    tableName: string
): number {
    const listConfig = getListConfig(config, tableName);
    if (listConfig?.pageSize !== undefined) {
        return listConfig.pageSize;
    }
    return getDefaults(config).pageSize;
}

/**
 * Get row expand config for a table
 */
export function getRowExpand(
    config: DibsAdminConfig | undefined,
    tableName: string
): RowExpandConfig | undefined {
    return getListConfig(config, tableName)?.rowExpand;
}

/**
 * Get image columns for a table
 */
export function getImageColumns(
    config: DibsAdminConfig | undefined,
    tableName: string
): string[] {
    return getListConfig(config, tableName)?.imageColumns ?? [];
}

// =============================================================================
// Sort/Filter Conversion Helpers
// =============================================================================

/**
 * Convert SortConfig to the internal Sort type
 */
export function sortConfigToSort(sortConfig: SortConfig): Sort {
    return {
        field: sortConfig.field,
        dir: sortConfig.direction === "desc" ? { tag: "Desc" } : { tag: "Asc" },
    };
}

/**
 * Convert internal Sort to SortConfig
 */
export function sortToSortConfig(sort: Sort): SortConfig {
    return {
        field: sort.field,
        direction: sort.dir.tag === "Desc" ? "desc" : "asc",
    };
}

/**
 * Map FilterConfig op string to internal FilterOp tag
 */
function filterOpToTag(op: FilterConfig["op"]): string {
    const mapping: Record<FilterConfig["op"], string> = {
        eq: "Eq",
        ne: "Ne",
        lt: "Lt",
        lte: "Lte",
        gt: "Gt",
        gte: "Gte",
        like: "Like",
        ilike: "ILike",
        null: "IsNull",
        notnull: "IsNotNull",
    };
    return mapping[op];
}

/**
 * Convert a FilterConfig value to internal Value type
 */
function filterValueToValue(value: unknown): Value {
    if (value === null || value === undefined) {
        return { tag: "Null" };
    }
    if (typeof value === "boolean") {
        return { tag: "Bool", value };
    }
    if (typeof value === "bigint") {
        return { tag: "I64", value };
    }
    if (typeof value === "number") {
        if (Number.isInteger(value)) {
            return { tag: "I32", value };
        }
        return { tag: "F64", value };
    }
    return { tag: "String", value: String(value) };
}

/**
 * Convert FilterConfig array to internal Filter array
 */
export function filterConfigsToFilters(filterConfigs: FilterConfig[]): Filter[] {
    return filterConfigs.map((fc) => ({
        field: fc.field,
        op: { tag: filterOpToTag(fc.op) } as Filter["op"],
        value: filterValueToValue(fc.value),
        values: [],
    }));
}

/**
 * Get default sort for a table (from config or fall back to created_at/PK desc)
 */
export function getDefaultSort(
    config: DibsAdminConfig | undefined,
    tableName: string,
    tableInfo: TableInfo | null
): Sort | null {
    const listConfig = getListConfig(config, tableName);

    // Use configured default sort
    if (listConfig?.defaultSort) {
        return sortConfigToSort(listConfig.defaultSort);
    }

    // Fall back to created_at desc if it exists
    if (tableInfo) {
        const createdAt = tableInfo.columns.find(
            (c) => c.name === "created_at" || c.name === "createdat"
        );
        if (createdAt) {
            return { field: createdAt.name, dir: { tag: "Desc" } };
        }

        // Fall back to primary key desc
        const pk = tableInfo.columns.find((c) => c.primary_key);
        if (pk) {
            return { field: pk.name, dir: { tag: "Desc" } };
        }
    }

    return null;
}

/**
 * Get default filters for a table
 */
export function getDefaultFilters(
    config: DibsAdminConfig | undefined,
    tableName: string
): Filter[] {
    const listConfig = getListConfig(config, tableName);
    if (listConfig?.defaultFilters) {
        return filterConfigsToFilters(listConfig.defaultFilters);
    }
    return [];
}

// =============================================================================
// Detail View Helpers
// =============================================================================

/**
 * Check if a field should be read-only in detail view
 */
export function isFieldReadOnly(
    colName: string,
    detailConfig: DetailConfig | undefined
): boolean {
    return detailConfig?.readOnly?.includes(colName) ?? false;
}

/**
 * Check if a field should be hidden in detail view
 */
export function isFieldHidden(
    colName: string,
    detailConfig: DetailConfig | undefined
): boolean {
    return detailConfig?.hidden?.includes(colName) ?? false;
}

/**
 * Check if relations section should be shown
 */
export function shouldShowRelations(
    detailConfig: DetailConfig | undefined
): boolean {
    return detailConfig?.showRelations !== false;
}

// =============================================================================
// Dashboard Helpers
// =============================================================================

/**
 * Check if dashboard is configured
 */
export function hasDashboard(config: DibsAdminConfig | undefined): boolean {
    return config?.dashboard !== undefined && config.dashboard.tiles.length > 0;
}
