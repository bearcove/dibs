<script lang="ts">
    import PlusIcon from "phosphor-svelte/lib/PlusIcon";
    import DynamicIcon from "../components/DynamicIcon.svelte";
    import DataTable from "../components/DataTable.svelte";
    import FilterInput from "../components/FilterInput.svelte";
    import Pagination from "../components/Pagination.svelte";
    import Breadcrumb from "../components/Breadcrumb.svelte";
    import { Button } from "@bearcove/dibs-admin/lib/ui";
    import { getAdminContext } from "../lib/admin-context.js";
    import { useRoute, useNavigate } from "@dvcol/svelte-simple-router/router";
    import {
        getTableLabel,
        getDisplayColumns,
        getPageSize,
        getListConfig,
        getRowExpand,
        getImageColumns,
    } from "@bearcove/dibs-admin/lib/config";
    import type {
        Row,
        Filter,
        Sort,
        Value,
        ListRequest,
        DibsError,
    } from "@bearcove/dibs-admin/types";

    const ctx = getAdminContext();
    const routeState = useRoute();
    const navigate = useNavigate();

    // Get table name from route params
    const tableName = $derived((routeState.route?.params as { table?: string })?.table ?? "");

    // Data state
    let rows = $state<Row[]>([]);
    let totalRows = $state<bigint | null>(null);
    let loading = $state(false);
    let error = $state<string | null>(null);

    // Query state
    let filters = $state<Filter[]>([]);
    let sort = $state<Sort | null>(null);
    let offset = $state(0);

    // Track previous table to detect changes
    let prevTable = $state("");

    // Derived
    let currentTable = $derived(ctx.schema?.tables.find((t) => t.name === tableName) ?? null);
    let columns = $derived(currentTable?.columns ?? []);
    let limit = $derived(getPageSize(ctx.config, tableName));
    let displayColumns = $derived(
        getDisplayColumns(columns, getListConfig(ctx.config, tableName)),
    );

    // Load data when table changes or query params change
    $effect(() => {
        if (ctx.schema && tableName) {
            // Reset state on table change
            if (tableName !== prevTable) {
                filters = [];
                sort = null;
                offset = 0;
                prevTable = tableName;
            }
            loadData();
        }
    });

    async function loadData() {
        if (!tableName) return;

        loading = true;
        error = null;
        try {
            const request: ListRequest = {
                table: tableName,
                filters,
                sort: sort ? [sort] : [],
                limit,
                offset,
                select: [],
            };
            const result = await ctx.client.list(request);
            if (result.ok) {
                rows = result.value.rows;
                totalRows = result.value.total ?? null;
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

    async function loadFkDisplayValues(loadedRows: Row[]) {
        if (!currentTable || !ctx.schema || loadedRows.length === 0) return;

        const fkValuesByTable = new Map<string, Set<string>>();

        for (const fk of currentTable.foreign_keys) {
            const colName = fk.columns[0];
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

        const newLookup = new Map(ctx.fkLookup);
        const fetchPromises: Promise<void>[] = [];

        for (const [refTableName, pkValues] of fkValuesByTable) {
            if (pkValues.size === 0) continue;

            const tableInfo = ctx.schema.tables.find((t) => t.name === refTableName);
            if (!tableInfo) continue;

            const pkCol = tableInfo.columns.find((c) => c.primary_key);
            if (!pkCol) continue;

            if (!newLookup.has(refTableName)) {
                newLookup.set(refTableName, new Map());
            }
            const tableCache = newLookup.get(refTableName)!;

            const uncachedPks = [...pkValues].filter((pk) => !tableCache.has(pk));
            if (uncachedPks.length === 0) continue;

            const inValues = uncachedPks.map((pk) => parsePkValue(pk, pkCol.sql_type));

            const labelCol = tableInfo.columns.find((c) => c.label);
            const displayCol =
                labelCol ??
                tableInfo.columns.find((c) =>
                    ["name", "title", "label", "display_name", "username", "email", "slug"].includes(
                        c.name.toLowerCase(),
                    ),
                );

            const selectCols = [pkCol.name];
            if (displayCol && displayCol.name !== pkCol.name) {
                selectCols.push(displayCol.name);
            }

            fetchPromises.push(
                ctx.client
                    .list({
                        table: refTableName,
                        filters: [
                            {
                                field: pkCol.name,
                                op: { tag: "In" },
                                value: { tag: "Null" },
                                values: inValues,
                            },
                        ],
                        sort: [],
                        limit: inValues.length,
                        offset: null,
                        select: selectCols,
                    })
                    .then((result) => {
                        if (result.ok) {
                            for (const row of result.value.rows) {
                                const pkField = row.fields.find((f) => f.name === pkCol.name);
                                if (pkField) {
                                    const pkStr = formatPkValue(pkField.value);
                                    tableCache.set(pkStr, row);
                                }
                            }
                        }
                    })
                    .catch((e) => {
                        console.error(`[FK lookup] ${refTableName} exception:`, e);
                    }),
            );
        }

        await Promise.all(fetchPromises);
        ctx.setFkLookup(newLookup);
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

    function handleSort(column: string) {
        if (sort && sort.field === column) {
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

    function openRow(row: Row) {
        if (!currentTable) return;
        const pkCol = currentTable.columns.find((c) => c.primary_key);
        if (!pkCol) return;
        const field = row.fields.find((f) => f.name === pkCol.name);
        if (!field) return;
        const pkStr = formatPkValue(field.value);
        navigate.push({ path: `${tableName}/${pkStr}` });
    }

    function openCreateDialog() {
        navigate.push({ path: `${tableName}/new` });
    }

    function navigateToFk(targetTable: string, pkValue: Value) {
        const pkStr = formatPkValue(pkValue);
        ctx.addBreadcrumb({
            table: targetTable,
            label: `${targetTable} #${pkStr}`,
            pkValue,
        });
        navigate.push({ path: `${targetTable}/${pkStr}` });
    }

    function navigateToBreadcrumb(index: number) {
        const crumbs = ctx.breadcrumbs;
        if (index < 0 || index >= crumbs.length) return;

        const entry = crumbs[index];
        ctx.setBreadcrumbs(crumbs.slice(0, index + 1));

        filters = [];
        sort = null;
        offset = 0;

        if (entry.pkValue) {
            const pkStr = formatPkValue(entry.pkValue);
            navigate.push({ path: `${entry.table}/${pkStr}` });
        } else {
            navigate.push({ path: entry.table });
        }
    }
</script>

<section class="table-section">
    {#if currentTable}
        <Breadcrumb entries={ctx.breadcrumbs} onNavigate={navigateToBreadcrumb} />

        <div class="table-header">
            <h2 class="table-title">
                <DynamicIcon name={currentTable.icon ?? "table"} size={20} class="table-icon" />
                {getTableLabel(ctx.config, tableName)}
            </h2>
            <Button onclick={openCreateDialog}>
                <PlusIcon size={16} />
                New
            </Button>
        </div>

        {#if error}
            <p class="error-message">{error}</p>
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
                onRowClick={openRow}
                table={currentTable}
                schema={ctx.schema ?? undefined}
                client={ctx.client}
                onFkClick={navigateToFk}
                fkLookup={ctx.fkLookup}
                timeMode={ctx.timeMode}
                rowExpand={getRowExpand(ctx.config, tableName)}
                imageColumns={getImageColumns(ctx.config, tableName)}
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
        <div class="status-message empty">Table not found</div>
    {/if}
</section>

<style>
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
