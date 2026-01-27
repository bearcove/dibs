<script lang="ts">
    import { untrack } from "svelte";
    import { Plus, House } from "phosphor-svelte";
    import DynamicIcon from "./components/DynamicIcon.svelte";
    import type {
        SquelServiceCaller,
        SchemaInfo,
        TableInfo,
        ColumnInfo,
        Row,
        Filter,
        Sort,
        Value,
        ListRequest,
        DibsError,
    } from "./types";
    import type { DibsAdminConfig } from "./types/config";
    import TableList from "./components/TableList.svelte";
    import DataTable from "./components/DataTable.svelte";
    import FilterInput from "./components/FilterInput.svelte";
    import Pagination from "./components/Pagination.svelte";
    import RowEditor from "./components/RowEditor.svelte";
    import RowDetail from "./components/RowDetail.svelte";
    import Breadcrumb from "./components/Breadcrumb.svelte";
    import Dashboard from "./components/Dashboard.svelte";
    import { Button, Tooltip } from "./lib/ui/index";
    import type { BreadcrumbEntry } from "./lib/fk-utils";
    import { createBreadcrumbLabel, getTableByName, getPkValue } from "./lib/fk-utils";
    import {
        isTableHidden,
        getTableLabel,
        getDisplayColumns,
        getPageSize,
        getDefaultSort,
        getDefaultFilters,
        hasDashboard,
        getListConfig,
        isColumnSortable,
        getRowExpand,
        getImageColumns,
    } from "./lib/config";

    interface Props {
        client: SquelServiceCaller;
        config?: DibsAdminConfig;
    }

    let { client, config }: Props = $props();

    // Schema state
    let schema = $state<SchemaInfo | null>(null);
    let selectedTable = $state<string | null>(null);
    let loading = $state(false);
    let error = $state<string | null>(null);

    // Dashboard state
    let showDashboard = $state(false);

    // Data state
    let rows = $state<Row[]>([]);
    let totalRows = $state<bigint | null>(null);

    // Query state
    let filters = $state<Filter[]>([]);
    let sort = $state<Sort | null>(null);
    let limit = $derived(selectedTable ? getPageSize(config, selectedTable) : 25);
    let offset = $state(0);

    // Router state - prevent infinite loops
    let isUpdatingFromHash = false;
    let isUpdatingHash = false;
    let schemaLoaded = false; // Prevent double-loading on mount

    // Editor state
    let editingRow = $state<Row | null>(null);
    let editingPk = $state<string | null>(null); // PK value as string for URL
    let isCreating = $state(false);
    let saving = $state(false);
    let deleting = $state(false);

    // Breadcrumb navigation state
    let breadcrumbs = $state<BreadcrumbEntry[]>([]);

    // Time display mode for timestamps
    let timeMode = $state<"relative" | "absolute">("relative");

    // FK lookup cache: table name -> pk string -> Row
    let fkLookup = $state<Map<string, Map<string, Row>>>(new Map());

    // Derived
    let currentTable = $derived(schema?.tables.find((t) => t.name === selectedTable) ?? null);
    let columns = $derived(currentTable?.columns ?? []);
    let displayColumns = $derived(
        getDisplayColumns(columns, getListConfig(config, selectedTable ?? "")),
    );

    // Filter tables to exclude hidden ones
    let visibleTables = $derived(
        schema?.tables.filter((t) => !isTableHidden(config, t.name)) ?? [],
    );

    // ==========================================================================
    // Hash-based routing
    // ==========================================================================

    // Op tags to URL-safe strings
    const opToString: Record<string, string> = {
        Eq: "eq",
        Ne: "ne",
        Lt: "lt",
        Lte: "lte",
        Gt: "gt",
        Gte: "gte",
        Like: "like",
        ILike: "ilike",
        IsNull: "null",
        IsNotNull: "notnull",
        In: "in",
    };
    const stringToOp: Record<string, string> = Object.fromEntries(
        Object.entries(opToString).map(([k, v]) => [v, k]),
    );

    function encodeHash(): string {
        // Dashboard view: #/
        if (showDashboard) {
            return "#/";
        }

        if (!selectedTable) return "";

        // Detail view: #/table/pk
        if (editingPk !== null) {
            return `#/${encodeURIComponent(selectedTable)}/${encodeURIComponent(editingPk)}`;
        }

        // Create view: #/table/new
        if (isCreating) {
            return `#/${encodeURIComponent(selectedTable)}/new`;
        }

        // List view: #/table?filters
        let hash = `#/${encodeURIComponent(selectedTable)}`;
        const params = new URLSearchParams();

        for (const f of filters) {
            const opStr = opToString[f.op.tag] ?? "eq";
            const key = `${f.field}__${opStr}`;
            if (f.op.tag === "In") {
                // Encode In values as comma-separated
                const vals = f.values.map((v) => encodeValue(v)).join(",");
                params.append(key, vals);
            } else if (f.op.tag === "IsNull" || f.op.tag === "IsNotNull") {
                params.append(key, "");
            } else {
                params.append(key, encodeValue(f.value));
            }
        }

        if (sort) {
            params.append("_sort", `${sort.field}__${sort.dir.tag.toLowerCase()}`);
        }
        if (offset > 0) {
            params.append("_offset", String(offset));
        }

        const qs = params.toString();
        return qs ? `${hash}?${qs}` : hash;
    }

    function encodeValue(v: Value): string {
        if (v.tag === "Null") return "";
        if (typeof v.value === "bigint") return v.value.toString();
        return String(v.value);
    }

    type DecodedHash = {
        table: string | null;
        filters: Filter[];
        sort: Sort | null;
        offset: number;
        rowPk: string | null; // If viewing a specific row
        isCreating: boolean; // If creating a new row
        isDashboard: boolean; // If showing dashboard
    };

    function decodeHash(hash: string, schemaInfo?: SchemaInfo | null): DecodedHash | null {
        if (!hash || !hash.startsWith("#/")) return null;

        const withoutHash = hash.slice(2); // Remove "#/"

        // Dashboard view: empty path after #/
        if (withoutHash === "" || withoutHash === "/") {
            return {
                table: null,
                filters: [],
                sort: null,
                offset: 0,
                rowPk: null,
                isCreating: false,
                isDashboard: true,
            };
        }

        const [pathPart, queryPart] = withoutHash.split("?");
        const pathSegments = pathPart.split("/").map((s) => decodeURIComponent(s));

        const table = pathSegments[0];
        if (!table) return null;

        // Check for detail view: #/table/pk or #/table/new
        let rowPk: string | null = null;
        let isCreating = false;
        if (pathSegments.length > 1) {
            const secondSegment = pathSegments[1];
            if (secondSegment === "new") {
                isCreating = true;
            } else if (secondSegment) {
                rowPk = secondSegment;
            }
        }

        // Find table info for type inference
        const tableInfo = schemaInfo?.tables.find((t) => t.name === table) ?? currentTable;

        const filters: Filter[] = [];
        let sort: Sort | null = null;
        let offset = 0;

        if (queryPart) {
            const params = new URLSearchParams(queryPart);
            for (const [key, value] of params.entries()) {
                if (key === "_sort") {
                    const [field, dir] = value.split("__");
                    if (field && dir) {
                        sort = { field, dir: dir === "desc" ? { tag: "Desc" } : { tag: "Asc" } };
                    }
                } else if (key === "_offset") {
                    offset = parseInt(value, 10) || 0;
                } else if (key.includes("__")) {
                    const [field, opStr] = key.split("__");
                    const opTag = stringToOp[opStr];
                    if (field && opTag) {
                        const op = { tag: opTag } as Filter["op"];
                        if (opTag === "In") {
                            const values = value
                                .split(",")
                                .map((v) => decodeValue(v, field, tableInfo));
                            filters.push({ field, op, value: { tag: "Null" }, values });
                        } else if (opTag === "IsNull" || opTag === "IsNotNull") {
                            filters.push({ field, op, value: { tag: "Null" }, values: [] });
                        } else {
                            filters.push({
                                field,
                                op,
                                value: decodeValue(value, field, tableInfo),
                                values: [],
                            });
                        }
                    }
                }
            }
        }

        return { table, filters, sort, offset, rowPk, isCreating, isDashboard: false };
    }

    function decodeValue(str: string, field: string, tableInfo?: TableInfo | null): Value {
        if (str === "") return { tag: "Null" };
        // Try to detect type from the table schema
        const col = tableInfo?.columns.find((c) => c.name === field);
        if (col) {
            const typeLower = col.sql_type.toLowerCase();
            if (typeLower.includes("int8") || typeLower === "bigint" || typeLower === "bigserial") {
                return { tag: "I64", value: BigInt(str) };
            }
            if (typeLower.includes("int")) {
                return { tag: "I32", value: parseInt(str, 10) };
            }
            if (typeLower.includes("bool")) {
                return { tag: "Bool", value: str === "true" || str === "1" };
            }
        }
        // Default to string
        return { tag: "String", value: str };
    }

    function updateHash() {
        if (isUpdatingFromHash) return;
        isUpdatingHash = true;
        const newHash = encodeHash();
        if (window.location.hash !== newHash) {
            window.history.pushState(null, "", newHash || window.location.pathname);
        }
        isUpdatingHash = false;
    }

    async function applyHash() {
        const decoded = decodeHash(window.location.hash, schema);
        if (!decoded) return;

        isUpdatingFromHash = true;

        // Handle dashboard view
        if (decoded.isDashboard) {
            showDashboard = true;
            selectedTable = null;
            editingPk = null;
            editingRow = null;
            isCreating = false;
            isUpdatingFromHash = false;
            return;
        }

        showDashboard = false;

        // Only apply if different from current state
        if (decoded.table !== selectedTable) {
            selectedTable = decoded.table;
            breadcrumbs = decoded.table ? [{ table: decoded.table, label: decoded.table }] : [];
        }

        filters = decoded.filters;
        sort = decoded.sort;
        offset = decoded.offset;

        // Handle detail view state
        if (decoded.rowPk !== null) {
            editingPk = decoded.rowPk;
            isCreating = false;
            // Load the specific row
            await loadRowByPk(decoded.rowPk);
        } else if (decoded.isCreating) {
            editingPk = null;
            editingRow = null;
            isCreating = true;
        } else {
            editingPk = null;
            editingRow = null;
            isCreating = false;
        }

        isUpdatingFromHash = false;
    }

    async function loadRowByPk(pkStr: string) {
        if (!selectedTable || !currentTable) return;

        const pkCol = currentTable.columns.find((c) => c.primary_key);
        if (!pkCol) return;

        const pkValue = parsePkValue(pkStr, pkCol.sql_type);

        try {
            const result = await client.get({
                table: selectedTable,
                pk: pkValue,
            });
            if (result.ok && result.value) {
                editingRow = result.value;
            } else {
                // Row not found, go back to list
                editingPk = null;
                editingRow = null;
            }
        } catch (e) {
            console.error("Failed to load row:", e);
            editingPk = null;
            editingRow = null;
        }
    }

    // Listen for popstate (back/forward)
    $effect(() => {
        function handlePopState() {
            if (isUpdatingHash) return;
            applyHash();
            loadData();
        }
        window.addEventListener("popstate", handlePopState);
        return () => window.removeEventListener("popstate", handlePopState);
    });

    // Update hash when state changes
    $effect(() => {
        // Depend on these values
        void selectedTable;
        void filters;
        void sort;
        void offset;
        void editingPk;
        void isCreating;
        void showDashboard;
        // Update hash (but not during initial load or when applying hash)
        if (schema && (selectedTable || showDashboard)) {
            updateHash();
        }
    });

    // Load schema on mount
    $effect(() => {
        untrack(() => loadSchema());
    });

    async function loadSchema() {
        if (schemaLoaded) return;
        schemaLoaded = true;

        loading = true;
        error = null;
        try {
            schema = await client.schema();
            if (schema.tables.length > 0) {
                // Check if there's a hash to apply
                const decoded = decodeHash(window.location.hash, schema);

                // Handle dashboard view from hash
                if (decoded?.isDashboard && hasDashboard(config)) {
                    showDashboard = true;
                    // No need to load table data for dashboard
                } else if (
                    decoded &&
                    decoded.table &&
                    schema.tables.some((t) => t.name === decoded.table)
                ) {
                    // Apply hash state for table view
                    isUpdatingFromHash = true;
                    showDashboard = false;
                    selectedTable = decoded.table;
                    filters = decoded.filters;
                    sort = decoded.sort;
                    offset = decoded.offset;
                    breadcrumbs = [{ table: decoded.table, label: decoded.table }];

                    // Handle detail/create views
                    if (decoded.rowPk !== null) {
                        editingPk = decoded.rowPk;
                        isCreating = false;
                    } else if (decoded.isCreating) {
                        editingPk = null;
                        isCreating = true;
                    }

                    isUpdatingFromHash = false;

                    // Load data first, then load specific row if needed
                    await loadData();

                    // If viewing a specific row, load it
                    if (decoded.rowPk !== null) {
                        await loadRowByPk(decoded.rowPk);
                    }
                } else if (hasDashboard(config)) {
                    // Default to dashboard if configured
                    showDashboard = true;
                } else {
                    // Default to first visible table
                    const firstVisible = schema.tables.find((t) => !isTableHidden(config, t.name));
                    if (firstVisible) {
                        selectTable(firstVisible.name);
                    }
                }
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            loading = false;
        }
    }

    async function loadData() {
        if (!selectedTable) return;

        loading = true;
        error = null;
        try {
            const request: ListRequest = {
                table: selectedTable,
                filters,
                sort: sort ? [sort] : [],
                limit,
                offset,
                select: [],
            };
            const result = await client.list(request);
            if (result.ok) {
                rows = result.value.rows;
                totalRows = result.value.total ?? null;

                // Load FK display values in the background
                loadFkDisplayValues(result.value.rows);
            } else {
                error = formatError(result.error);
                rows = [];
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
            rows = [];
        } finally {
            loading = false;
        }
    }

    // Load display values for FK columns
    async function loadFkDisplayValues(loadedRows: Row[]) {
        if (!currentTable || !schema || loadedRows.length === 0) return;

        console.log(
            `[FK lookup] Starting for table ${currentTable.name}, FKs:`,
            currentTable.foreign_keys.map((fk) => `${fk.columns[0]} -> ${fk.references_table}`),
        );

        // Collect FK values grouped by referenced table
        const fkValuesByTable = new Map<string, Set<string>>();

        for (const fk of currentTable.foreign_keys) {
            const colName = fk.columns[0]; // For simplicity, handle single-column FKs
            const refTable = fk.references_table;

            if (!fkValuesByTable.has(refTable)) {
                fkValuesByTable.set(refTable, new Set());
            }

            for (const row of loadedRows) {
                const field = row.fields.find((f) => f.name === colName);
                if (field && field.value.tag !== "Null") {
                    const pkStr = formatPkValue(field.value);
                    fkValuesByTable.get(refTable)!.add(pkStr);
                }
            }
        }

        console.log(
            `[FK lookup] Collected values:`,
            Object.fromEntries([...fkValuesByTable.entries()].map(([k, v]) => [k, [...v]])),
        );

        // Fetch rows from each referenced table using IN filter (single query per table)
        const newLookup = new Map(fkLookup);

        const fetchPromises: Promise<void>[] = [];

        for (const [tableName, pkValues] of fkValuesByTable) {
            if (pkValues.size === 0) continue;

            const tableInfo = schema.tables.find((t) => t.name === tableName);
            if (!tableInfo) continue;

            const pkCol = tableInfo.columns.find((c) => c.primary_key);
            if (!pkCol) continue;

            if (!newLookup.has(tableName)) {
                newLookup.set(tableName, new Map());
            }
            const tableCache = newLookup.get(tableName)!;

            // Filter out already-cached values
            const uncachedPks = [...pkValues].filter((pk) => !tableCache.has(pk));
            if (uncachedPks.length === 0) continue;

            // Convert to Value array for IN filter
            const inValues = uncachedPks.map((pk) => parsePkValue(pk, pkCol.sql_type));

            // Find the label column for this table
            const labelCol = tableInfo.columns.find((c) => c.label);
            const displayCol =
                labelCol ??
                tableInfo.columns.find((c) =>
                    [
                        "name",
                        "title",
                        "label",
                        "display_name",
                        "username",
                        "email",
                        "slug",
                    ].includes(c.name.toLowerCase()),
                );

            // Only select PK and display columns to optimize the query
            const selectCols = [pkCol.name];
            if (displayCol && displayCol.name !== pkCol.name) {
                selectCols.push(displayCol.name);
            }

            // Single batch fetch using IN filter
            const startTime = performance.now();
            fetchPromises.push(
                client
                    .list({
                        table: tableName,
                        filters: [
                            {
                                field: pkCol.name,
                                op: { tag: "In" },
                                value: { tag: "Null" }, // Not used for In
                                values: inValues,
                            },
                        ],
                        sort: [],
                        limit: inValues.length,
                        offset: null,
                        select: selectCols,
                    })
                    .then((result) => {
                        const elapsed = performance.now() - startTime;
                        console.log(
                            `[FK lookup] ${tableName}: fetched ${result.ok ? result.value.rows.length : 0} rows in ${elapsed.toFixed(0)}ms`,
                        );
                        if (result.ok) {
                            // Add each fetched row to cache
                            for (const row of result.value.rows) {
                                const pkField = row.fields.find((f) => f.name === pkCol.name);
                                if (pkField) {
                                    const pkStr = formatPkValue(pkField.value);
                                    tableCache.set(pkStr, row);
                                }
                            }
                        } else {
                            console.error(`[FK lookup] ${tableName} error:`, result.error);
                        }
                    })
                    .catch((e) => {
                        console.error(`[FK lookup] ${tableName} exception:`, e);
                    }),
            );
        }

        // Wait for all fetches to complete (one per referenced table)
        await Promise.all(fetchPromises);
        fkLookup = newLookup;
    }

    function formatPkValue(value: Value): string {
        if (value.tag === "Null") return "";
        if (typeof value.value === "bigint") return value.value.toString();
        return String(value.value);
    }

    function parsePkValue(str: string, sqlType: string): Value {
        const typeLower = sqlType.toLowerCase();
        if (typeLower.includes("int8") || typeLower === "bigint" || typeLower === "bigserial") {
            return { tag: "I64", value: BigInt(str) };
        }
        if (typeLower.includes("int")) {
            return { tag: "I32", value: parseInt(str, 10) };
        }
        return { tag: "String", value: str };
    }

    function formatError(err: DibsError): string {
        if (err.tag === "MigrationFailed") {
            return `${err.tag}: ${err.value.message}`;
        }
        return `${err.tag}: ${err.value}`;
    }

    function selectTable(tableName: string, resetBreadcrumbs = true) {
        showDashboard = false;
        selectedTable = tableName;
        filters = [];
        sort = null;
        offset = 0;
        // Clear detail/create view state
        editingPk = null;
        editingRow = null;
        isCreating = false;
        if (resetBreadcrumbs) {
            breadcrumbs = [{ table: tableName, label: tableName }];
        }
        loadData();
    }

    function goToDashboard() {
        showDashboard = true;
        selectedTable = null;
        editingPk = null;
        editingRow = null;
        isCreating = false;
        breadcrumbs = [];
    }

    // Navigate to an FK target
    async function navigateToFk(targetTable: string, pkValue: Value) {
        if (!schema) return;

        const table = getTableByName(schema, targetTable);
        if (!table) return;

        // Find the PK column
        const pkCol = table.columns.find((c) => c.primary_key);
        if (!pkCol) return;

        // Add to breadcrumbs with a label we'll update after loading
        const newEntry: BreadcrumbEntry = {
            table: targetTable,
            label: `${targetTable} #${pkValue.tag !== "Null" ? (typeof pkValue.value === "bigint" ? pkValue.value.toString() : String(pkValue.value)) : "?"}`,
            pkValue,
        };

        breadcrumbs = [...breadcrumbs, newEntry];

        // Navigate to the table with a filter for the specific row
        selectedTable = targetTable;
        filters = [
            {
                field: pkCol.name,
                op: { tag: "Eq" },
                value: pkValue,
                values: [],
            },
        ];
        sort = null;
        offset = 0;

        await loadData();

        // Update the breadcrumb label with the actual display value
        if (rows.length > 0 && currentTable) {
            const label = createBreadcrumbLabel(currentTable, rows[0]);
            breadcrumbs = breadcrumbs.map((b, i) =>
                i === breadcrumbs.length - 1 ? { ...b, label } : b,
            );
        }
    }

    // Navigate back via breadcrumb
    function navigateToBreadcrumb(index: number) {
        if (index < 0 || index >= breadcrumbs.length) return;

        const entry = breadcrumbs[index];
        breadcrumbs = breadcrumbs.slice(0, index + 1);

        selectedTable = entry.table;

        // If there's a PK value, filter to that row; otherwise show all
        if (entry.pkValue) {
            const table = schema?.tables.find((t) => t.name === entry.table);
            const pkCol = table?.columns.find((c) => c.primary_key);
            if (pkCol) {
                filters = [
                    {
                        field: pkCol.name,
                        op: { tag: "Eq" },
                        value: entry.pkValue,
                        values: [],
                    },
                ];
            } else {
                filters = [];
            }
        } else {
            filters = [];
        }

        sort = null;
        offset = 0;
        loadData();
    }

    function handleSort(column: string) {
        if (sort && sort.field === column) {
            // Toggle direction
            sort = {
                field: column,
                dir: sort.dir.tag === "Asc" ? { tag: "Desc" } : { tag: "Asc" },
            };
        } else {
            sort = { field: column, dir: { tag: "Asc" } };
        }
        offset = 0;
        loadData();
    }

    function addFilter(filter: Filter) {
        filters = [...filters, filter];
        offset = 0;
        loadData();
    }

    function removeFilter(index: number) {
        filters = filters.filter((_, i) => i !== index);
        offset = 0;
        loadData();
    }

    function clearFilters() {
        filters = [];
        offset = 0;
        loadData();
    }

    function setFilters(newFilters: Filter[]) {
        filters = newFilters;
        offset = 0;
        loadData();
    }

    function nextPage() {
        offset += limit;
        loadData();
    }

    function prevPage() {
        offset = Math.max(0, offset - limit);
        loadData();
    }

    function openEditor(row: Row) {
        editingRow = row;
        isCreating = false;
        // Set the pk for URL
        const pk = getPrimaryKeyValue(row);
        editingPk = pk ? formatPkValue(pk) : null;
    }

    function openCreateDialog() {
        editingRow = null;
        editingPk = null;
        isCreating = true;
    }

    function closeEditor() {
        editingRow = null;
        editingPk = null;
        isCreating = false;
    }

    function getPrimaryKeyValue(row: Row): Value | null {
        if (!currentTable) return null;
        const pkCol = currentTable.columns.find((c) => c.primary_key);
        if (!pkCol) return null;
        const field = row.fields.find((f) => f.name === pkCol.name);
        return field?.value ?? null;
    }

    async function saveRow(data: Row, dirtyFields?: Set<string>) {
        if (!selectedTable) return;

        saving = true;
        error = null;

        try {
            if (isCreating) {
                const result = await client.create({
                    table: selectedTable,
                    data,
                });
                if (!result.ok) {
                    error = formatError(result.error);
                    return;
                }
            } else if (editingRow) {
                const pk = getPrimaryKeyValue(editingRow);
                if (!pk) {
                    error = "Could not determine primary key";
                    return;
                }

                // For updates, only send the modified fields
                const updateData: Row = dirtyFields
                    ? { fields: data.fields.filter((f) => dirtyFields.has(f.name)) }
                    : data;

                const result = await client.update({
                    table: selectedTable,
                    pk,
                    data: updateData,
                });
                if (!result.ok) {
                    error = formatError(result.error);
                    return;
                }
            }
            closeEditor();
            loadData();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            saving = false;
        }
    }

    // Save a single field (for inline editing)
    async function saveField(fieldName: string, newValue: Value) {
        if (!selectedTable || !editingRow) {
            throw new Error("No row being edited");
        }

        const pk = getPrimaryKeyValue(editingRow);
        if (!pk) {
            throw new Error("Could not determine primary key");
        }

        const updateData: Row = {
            fields: [{ name: fieldName, value: newValue }],
        };

        const result = await client.update({
            table: selectedTable,
            pk,
            data: updateData,
        });

        if (!result.ok) {
            throw new Error(formatError(result.error));
        }

        // Update the local editingRow with the new value
        if (editingRow) {
            editingRow = {
                fields: editingRow.fields.map((f) =>
                    f.name === fieldName ? { name: fieldName, value: newValue } : f,
                ),
            };
        }
    }

    // Navigate to a related record (opens detail view directly)
    async function handleRelatedNavigate(tableName: string, pkValue: Value) {
        if (!schema) return;

        const table = getTableByName(schema, tableName);
        if (!table) return;

        const pkCol = table.columns.find((c) => c.primary_key);
        if (!pkCol) return;

        // Add breadcrumb entry
        const pkStr = formatPkValue(pkValue);
        const newEntry: BreadcrumbEntry = {
            table: tableName,
            label: `${tableName} #${pkStr}`,
            pkValue,
        };
        breadcrumbs = [...breadcrumbs, newEntry];

        // Switch table and load the specific row's detail view
        selectedTable = tableName;
        filters = [];
        sort = null;
        offset = 0;
        editingPk = pkStr;
        isCreating = false;

        // Load the row data
        await loadRowByPk(pkStr);

        // Update breadcrumb with display label
        if (editingRow && currentTable) {
            const label = createBreadcrumbLabel(currentTable, editingRow);
            breadcrumbs = breadcrumbs.map((b, i) =>
                i === breadcrumbs.length - 1 ? { ...b, label } : b,
            );
        }
    }

    async function deleteRow() {
        if (!selectedTable || !editingRow) return;

        const pk = getPrimaryKeyValue(editingRow);
        if (!pk) {
            error = "Could not determine primary key";
            return;
        }

        deleting = true;
        error = null;

        try {
            const result = await client.delete({
                table: selectedTable,
                pk,
            });
            if (!result.ok) {
                error = formatError(result.error);
                return;
            }
            closeEditor();
            loadData();
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            deleting = false;
        }
    }
</script>

<Tooltip.Provider>
    <div class="admin-root">
        {#if loading && !schema}
            <div class="loading-state">Loading schema...</div>
        {:else if schema}
            <div class="admin-layout">
                <TableList
                    tables={visibleTables}
                    selected={selectedTable}
                    onSelect={selectTable}
                    {config}
                    showDashboardButton={hasDashboard(config)}
                    onDashboard={goToDashboard}
                    dashboardActive={showDashboard}
                    {timeMode}
                    onTimeModeChange={(mode) => (timeMode = mode)}
                />

                {#if showDashboard && config?.dashboard}
                    <!-- Dashboard view -->
                    <Dashboard {config} {schema} {client} onSelectTable={selectTable} />
                {:else if editingRow && currentTable}
                    <!-- Detail view with inline editing -->
                    <RowDetail
                        {columns}
                        row={editingRow}
                        table={currentTable}
                        {schema}
                        {client}
                        tableName={selectedTable ?? ""}
                        {config}
                        onFieldSave={saveField}
                        onDelete={deleteRow}
                        onClose={closeEditor}
                        {deleting}
                        onNavigate={handleRelatedNavigate}
                    />
                {:else if isCreating}
                    <!-- Create new row form -->
                    <RowEditor
                        {columns}
                        row={null}
                        onSave={saveRow}
                        onClose={closeEditor}
                        {saving}
                        table={currentTable ?? undefined}
                        schema={schema ?? undefined}
                        {client}
                        fullscreen={true}
                        tableName={selectedTable ?? ""}
                    />
                {:else}
                    <!-- Table list view -->
                    <section class="table-section">
                        {#if selectedTable && currentTable}
                            <Breadcrumb entries={breadcrumbs} onNavigate={navigateToBreadcrumb} />

                            <div class="table-header">
                                <h2 class="table-title">
                                    <DynamicIcon
                                        name={currentTable.icon ?? "table"}
                                        size={20}
                                        class="table-icon"
                                    />
                                    {selectedTable}
                                </h2>
                                <Button onclick={openCreateDialog}>
                                    <Plus size={16} />
                                    New
                                </Button>
                            </div>

                            {#if error}
                                <p class="error-message">
                                    {error}
                                </p>
                            {/if}

                            <FilterInput {columns} {filters} onFiltersChange={setFilters} />

                            {#if loading}
                                <div class="status-message">Loading...</div>
                            {:else if rows.length === 0}
                                <div class="status-message empty">No rows found.</div>
                            {:else}
                                <DataTable
                                    columns={displayColumns}
                                    {rows}
                                    {sort}
                                    onSort={handleSort}
                                    onRowClick={openEditor}
                                    table={currentTable}
                                    {schema}
                                    {client}
                                    onFkClick={navigateToFk}
                                    {fkLookup}
                                    {timeMode}
                                    rowExpand={getRowExpand(config, selectedTable ?? "")}
                                    imageColumns={getImageColumns(config, selectedTable ?? "")}
                                />

                                <Pagination
                                    {offset}
                                    {limit}
                                    rowCount={rows.length}
                                    total={totalRows}
                                    onPrev={prevPage}
                                    onNext={nextPage}
                                />
                            {/if}
                        {:else}
                            <div class="status-message empty">Select a table</div>
                        {/if}
                    </section>
                {/if}
            </div>
        {:else if error}
            <p class="error-message standalone">
                {error}
            </p>
        {/if}
    </div>
</Tooltip.Provider>

<style>
    .admin-root {
        height: 100%;
        min-height: 100vh;
        background-color: var(--background);
        color: var(--foreground);
    }

    .loading-state {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: 2rem;
        color: var(--muted-foreground);
    }

    .admin-layout {
        height: 100%;
        min-height: 100vh;
        display: grid;
        grid-template-columns: 220px 1fr;
    }

    .table-section {
        padding: 1.5rem;
        overflow: auto;
        display: flex;
        flex-direction: column;
        max-height: 100vh;
        max-width: 72rem;
    }

    @media (min-width: 768px) {
        .table-section {
            padding: 2rem;
        }
    }

    .table-header {
        display: flex;
        justify-content: space-between;
        align-items: center;
        margin-bottom: 1.5rem;
    }

    .table-title {
        font-size: 1.125rem;
        font-weight: 500;
        color: var(--foreground);
        text-transform: uppercase;
        letter-spacing: 0.05em;
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    :global(.table-icon) {
        opacity: 0.7;
    }

    .error-message {
        color: var(--destructive);
        margin-bottom: 1.5rem;
        font-size: 0.875rem;
    }

    .error-message.standalone {
        padding: 2rem;
        margin-bottom: 0;
    }

    .status-message {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        color: var(--muted-foreground);
    }

    .status-message.empty {
        color: oklch(from var(--muted-foreground) l c h / 0.6);
    }
</style>
