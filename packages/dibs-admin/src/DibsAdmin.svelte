<script lang="ts">
    import { untrack } from "svelte";
    import PlusIcon from "phosphor-svelte/lib/PlusIcon";
    import HouseIcon from "phosphor-svelte/lib/HouseIcon";
    import DynamicIcon from "./components/DynamicIcon.svelte";
    import { useRouter } from "@dvcol/svelte-simple-router";
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
    } from "@bearcove/dibs-admin/types";
    import type { DibsAdminConfig } from "@bearcove/dibs-admin/types/config";
    import TableList from "./components/TableList.svelte";
    import DataTable from "./components/DataTable.svelte";
    import FilterInput from "./components/FilterInput.svelte";
    import Pagination from "./components/Pagination.svelte";
    import RowEditor from "./components/RowEditor.svelte";
    import RowDetail from "./components/RowDetail.svelte";
    import Breadcrumb from "./components/Breadcrumb.svelte";
    import Dashboard from "./components/Dashboard.svelte";
    import { Button, Tooltip } from "@bearcove/dibs-admin/lib/ui";
    import type { BreadcrumbEntry } from "@bearcove/dibs-admin/lib/fk-utils";
    import {
        createBreadcrumbLabel,
        getTableByName,
        getPkValue,
    } from "@bearcove/dibs-admin/lib/fk-utils";
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
    } from "@bearcove/dibs-admin/lib/config";
    import { schemaCache } from "./lib/schema-cache.js";

    interface RouteNames {
        dashboard?: string;   // Route for /admin (dashboard)
        table?: string;       // Route for /admin/:table
        tableNew?: string;    // Route for /admin/:table/new
        tableRow?: string;    // Route for /admin/:table/:pk
    }

    interface Props {
        client: SquelServiceCaller;
        config?: DibsAdminConfig;
        routes?: RouteNames;
    }

    let { client, config, routes }: Props = $props();

    // Get router for navigation
    const router = useRouter();

    // Route params - the source of truth for navigation state
    const routeParams = $derived(router?.current?.route?.params as { table?: string; pk?: string } | undefined);

    // Derived from route params
    const selectedTable = $derived(routeParams?.table ?? null);
    const showDashboard = $derived(!routeParams?.table);
    const editingPk = $derived(
        routeParams?.pk && routeParams.pk !== "new" ? routeParams.pk : null
    );
    const isCreating = $derived(routeParams?.pk === "new");

    // Schema state - initialize from cache if available
    let schema = $state<SchemaInfo | null>(schemaCache.get(client) ?? null);
    let loading = $state(false);
    let error = $state<string | null>(null);

    // Data state
    let rows = $state<Row[]>([]);
    let totalRows = $state<bigint | null>(null);

    // Query state (local, could be query params in future)
    let filters = $state<Filter[]>([]);
    let sort = $state<Sort | null>(null);
    let limit = $derived(selectedTable ? getPageSize(config, selectedTable) : 25);
    let offset = $state(0);

    // Prevent double-loading on mount
    let schemaLoaded = false;

    // Editor state
    let editingRow = $state<Row | null>(null);
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
    // Router-based navigation
    // ==========================================================================

    // Navigate to dashboard
    function navigateToDashboard() {
        if (routes?.dashboard) {
            router?.push({ name: routes.dashboard });
        }
    }

    // Navigate to table list view
    function navigateToTable(table: string) {
        if (routes?.table) {
            router?.push({ name: routes.table, params: { table } });
        }
    }

    // Navigate to new row view
    function navigateToNewRow(table: string) {
        if (routes?.tableNew) {
            router?.push({ name: routes.tableNew, params: { table } });
        }
    }

    // Navigate to row detail view
    function navigateToRow(table: string, pk: string) {
        if (routes?.tableRow) {
            router?.push({ name: routes.tableRow, params: { table, pk } });
        }
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
                editingRow = null;
                navigateToTable(selectedTable);
            }
        } catch (e) {
            console.error("Failed to load row:", e);
            editingRow = null;
            navigateToTable(selectedTable);
        }
    }

    // Load data when route changes (selectedTable is derived from route)
    $effect(() => {
        if (schema && selectedTable) {
            loadData();
        }
    });

    // Load row when viewing a specific row
    $effect(() => {
        if (schema && selectedTable && editingPk && !editingRow) {
            loadRowByPk(editingPk);
        }
    });

    // Load schema on mount
    $effect(() => {
        untrack(() => loadSchema());
    });

    async function loadSchema() {
        if (schemaLoaded) return;
        schemaLoaded = true;

        // Use cached schema if available
        const cached = schemaCache.get(client);
        if (cached) {
            schema = cached;
        } else {
            loading = true;
            error = null;
            try {
                schema = await client.schema();
                schemaCache.set(client, schema);
            } catch (e) {
                error = e instanceof Error ? e.message : String(e);
            } finally {
                loading = false;
            }
            if (!schema) return;
        }

        // Initialize breadcrumbs if we have a table from the route
        if (selectedTable) {
            breadcrumbs = [{ table: selectedTable, label: selectedTable }];
        }

        // If no table selected and no dashboard, navigate to first table
        if (!selectedTable && !hasDashboard(config) && schema.tables.length > 0) {
            const firstVisible = schema.tables.find((t) => !isTableHidden(config, t.name));
            if (firstVisible) {
                navigateToTable(firstVisible.name);
            }
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
        // Reset local query state
        filters = [];
        sort = null;
        offset = 0;
        editingRow = null;
        if (resetBreadcrumbs) {
            breadcrumbs = [{ table: tableName, label: tableName }];
        }
        // Navigate via router
        navigateToTable(tableName);
    }

    function goToDashboard() {
        editingRow = null;
        breadcrumbs = [];
        navigateToDashboard();
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

        // Navigate to the row detail view
        const pkStr = pkValue.tag !== "Null"
            ? (typeof pkValue.value === "bigint" ? pkValue.value.toString() : String(pkValue.value))
            : "";
        navigateToRow(targetTable, pkStr);
    }

    // Navigate back via breadcrumb
    function navigateToBreadcrumb(index: number) {
        if (index < 0 || index >= breadcrumbs.length) return;

        const entry = breadcrumbs[index];
        breadcrumbs = breadcrumbs.slice(0, index + 1);

        // Reset query state
        filters = [];
        sort = null;
        offset = 0;

        // Navigate via router
        if (entry.pkValue) {
            const pkStr = entry.pkValue.tag !== "Null"
                ? (typeof entry.pkValue.value === "bigint" ? entry.pkValue.value.toString() : String(entry.pkValue.value))
                : "";
            navigateToRow(entry.table, pkStr);
        } else {
            navigateToTable(entry.table);
        }
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
        // Navigate to row detail view
        if (selectedTable) {
            const pk = getPrimaryKeyValue(row);
            if (pk) {
                navigateToRow(selectedTable, formatPkValue(pk));
            }
        }
    }

    function openCreateDialog() {
        editingRow = null;
        if (selectedTable) {
            navigateToNewRow(selectedTable);
        }
    }

    function closeEditor() {
        editingRow = null;
        if (selectedTable) {
            navigateToTable(selectedTable);
        }
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
    function handleRelatedNavigate(tableName: string, pkValue: Value) {
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

        // Reset query state
        filters = [];
        sort = null;
        offset = 0;

        // Navigate to row detail view
        navigateToRow(tableName, pkStr);
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
                                    {getTableLabel(config, selectedTable ?? "")}
                                </h2>
                                <Button onclick={openCreateDialog}>
                                    <PlusIcon size={16} />
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
        display: grid;
        grid-template-columns: 280px 1fr;
        min-height: 100vh;
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
