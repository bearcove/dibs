<script lang="ts">
    import { Plus } from "phosphor-svelte";
    import type {
        SquelClient,
        SchemaInfo,
        TableInfo,
        ColumnInfo,
        Row,
        Filter,
        Sort,
        Value,
        ListRequest,
    } from "./types.js";
    import TableList from "./components/TableList.svelte";
    import DataTable from "./components/DataTable.svelte";
    import FilterBar from "./components/FilterBar.svelte";
    import Pagination from "./components/Pagination.svelte";
    import RowEditor from "./components/RowEditor.svelte";
    import Breadcrumb from "./components/Breadcrumb.svelte";
    import { Button } from "./lib/components/ui/index.js";
    import type { BreadcrumbEntry } from "./lib/fk-utils.js";
    import { createBreadcrumbLabel, getTableByName, getPkValue } from "./lib/fk-utils.js";

    interface Props {
        client: SquelClient;
        databaseUrl: string;
    }

    let { client, databaseUrl }: Props = $props();

    // Schema state
    let schema = $state<SchemaInfo | null>(null);
    let selectedTable = $state<string | null>(null);
    let loading = $state(false);
    let error = $state<string | null>(null);

    // Data state
    let rows = $state<Row[]>([]);
    let totalRows = $state<bigint | null>(null);

    // Query state
    let filters = $state<Filter[]>([]);
    let sort = $state<Sort | null>(null);
    let limit = $state(25);
    let offset = $state(0);

    // Editor state
    let editingRow = $state<Row | null>(null);
    let isCreating = $state(false);
    let saving = $state(false);
    let deleting = $state(false);

    // Breadcrumb navigation state
    let breadcrumbs = $state<BreadcrumbEntry[]>([]);

    // FK lookup cache: table name -> pk string -> Row
    let fkLookup = $state<Map<string, Map<string, Row>>>(new Map());

    // Derived
    let currentTable = $derived(schema?.tables.find((t) => t.name === selectedTable) ?? null);
    let columns = $derived(currentTable?.columns ?? []);

    // Load schema on mount
    $effect(() => {
        loadSchema();
    });

    async function loadSchema() {
        loading = true;
        error = null;
        try {
            schema = await client.schema();
            if (schema.tables.length > 0) {
                selectTable(schema.tables[0].name);
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
                database_url: databaseUrl,
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

        // Collect FK values grouped by referenced table
        const fkValuesByTable = new Map<string, Set<string>>();

        for (const fk of currentTable.foreign_keys) {
            const colName = fk.columns[0]; // For simplicity, handle single-column FKs
            const refTable = fk.references_table;

            if (!fkValuesByTable.has(refTable)) {
                fkValuesByTable.set(refTable, new Set());
            }

            for (const row of loadedRows) {
                const field = row.fields.find(f => f.name === colName);
                if (field && field.value.tag !== "Null") {
                    const pkStr = formatPkValue(field.value);
                    fkValuesByTable.get(refTable)!.add(pkStr);
                }
            }
        }

        // Fetch rows from each referenced table using IN filter (single query per table)
        const newLookup = new Map(fkLookup);

        const fetchPromises: Promise<void>[] = [];

        for (const [tableName, pkValues] of fkValuesByTable) {
            if (pkValues.size === 0) continue;

            const tableInfo = schema.tables.find(t => t.name === tableName);
            if (!tableInfo) continue;

            const pkCol = tableInfo.columns.find(c => c.primary_key);
            if (!pkCol) continue;

            if (!newLookup.has(tableName)) {
                newLookup.set(tableName, new Map());
            }
            const tableCache = newLookup.get(tableName)!;

            // Filter out already-cached values
            const uncachedPks = [...pkValues].filter(pk => !tableCache.has(pk));
            if (uncachedPks.length === 0) continue;

            // Convert to Value array for IN filter
            const inValues = uncachedPks.map(pk => parsePkValue(pk, pkCol.sql_type));

            // Single batch fetch using IN filter
            fetchPromises.push(
                client.list({
                    database_url: databaseUrl,
                    table: tableName,
                    filters: [{
                        field: pkCol.name,
                        op: { tag: "In" },
                        value: { tag: "Null" }, // Not used for In
                        values: inValues,
                    }],
                    sort: [],
                    limit: inValues.length,
                    offset: null,
                    select: [],
                }).then(result => {
                    if (result.ok) {
                        // Add each fetched row to cache
                        for (const row of result.value.rows) {
                            const pkField = row.fields.find(f => f.name === pkCol.name);
                            if (pkField) {
                                const pkStr = formatPkValue(pkField.value);
                                tableCache.set(pkStr, row);
                            }
                        }
                    }
                }).catch(() => {
                    // Ignore fetch errors for display values
                })
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

    function formatError(err: { tag: string; value: string }): string {
        return `${err.tag}: ${err.value}`;
    }

    function selectTable(tableName: string, resetBreadcrumbs = true) {
        selectedTable = tableName;
        filters = [];
        sort = null;
        offset = 0;
        if (resetBreadcrumbs) {
            breadcrumbs = [{ table: tableName, label: tableName }];
        }
        loadData();
    }

    // Navigate to an FK target
    async function navigateToFk(targetTable: string, pkValue: Value) {
        if (!schema) return;

        const table = getTableByName(schema, targetTable);
        if (!table) return;

        // Find the PK column
        const pkCol = table.columns.find(c => c.primary_key);
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
        filters = [{
            field: pkCol.name,
            op: { tag: "Eq" },
            value: pkValue,
            values: [],
        }];
        sort = null;
        offset = 0;

        await loadData();

        // Update the breadcrumb label with the actual display value
        if (rows.length > 0 && currentTable) {
            const label = createBreadcrumbLabel(currentTable, rows[0]);
            breadcrumbs = breadcrumbs.map((b, i) =>
                i === breadcrumbs.length - 1 ? { ...b, label } : b
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
            const table = schema?.tables.find(t => t.name === entry.table);
            const pkCol = table?.columns.find(c => c.primary_key);
            if (pkCol) {
                filters = [{
                    field: pkCol.name,
                    op: { tag: "Eq" },
                    value: entry.pkValue,
                    values: [],
                }];
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
    }

    function openCreateDialog() {
        editingRow = null;
        isCreating = true;
    }

    function closeEditor() {
        editingRow = null;
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
                    database_url: databaseUrl,
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
                    ? { fields: data.fields.filter(f => dirtyFields.has(f.name)) }
                    : data;

                const result = await client.update({
                    database_url: databaseUrl,
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
                database_url: databaseUrl,
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

<div class="h-full bg-neutral-950 text-neutral-100">
    {#if loading && !schema}
        <div class="flex items-center justify-center h-full p-8 text-neutral-500">
            Loading schema...
        </div>
    {:else if schema}
        <div class="grid grid-cols-[200px_1fr] h-full">
            <TableList tables={schema.tables} selected={selectedTable} onSelect={selectTable} />

            <section class="p-8 overflow-auto flex flex-col">
                {#if selectedTable && currentTable}
                    <Breadcrumb entries={breadcrumbs} onNavigate={navigateToBreadcrumb} />

                    <div class="flex justify-between items-center mb-8">
                        <h2 class="text-lg font-medium text-white uppercase tracking-wide">{selectedTable}</h2>
                        <Button onclick={openCreateDialog}>
                            <Plus size={16} />
                            New
                        </Button>
                    </div>

                    {#if error}
                        <p class="text-red-400 mb-6 text-sm">
                            {error}
                        </p>
                    {/if}

                    <FilterBar
                        {columns}
                        {filters}
                        onAddFilter={addFilter}
                        onRemoveFilter={removeFilter}
                        onClearFilters={clearFilters}
                    />

                    {#if loading}
                        <div class="flex-1 flex items-center justify-center text-neutral-500">
                            Loading...
                        </div>
                    {:else if rows.length === 0}
                        <div class="flex-1 flex items-center justify-center text-neutral-600">
                            No rows found.
                        </div>
                    {:else}
                        <DataTable
                            {columns}
                            {rows}
                            {sort}
                            onSort={handleSort}
                            onRowClick={openEditor}
                            table={currentTable}
                            {schema}
                            {client}
                            {databaseUrl}
                            onFkClick={navigateToFk}
                            {fkLookup}
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
                    <div class="flex-1 flex items-center justify-center text-neutral-600">
                        Select a table
                    </div>
                {/if}
            </section>
        </div>
    {:else if error}
        <p class="text-red-400 p-8 text-sm">
            {error}
        </p>
    {/if}

    {#if editingRow || isCreating}
        <RowEditor
            {columns}
            row={editingRow}
            onSave={saveRow}
            onDelete={editingRow ? deleteRow : undefined}
            onClose={closeEditor}
            {saving}
            {deleting}
            table={currentTable ?? undefined}
            schema={schema ?? undefined}
            {client}
            {databaseUrl}
        />
    {/if}
</div>
