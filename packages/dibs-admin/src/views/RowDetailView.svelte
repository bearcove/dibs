<script lang="ts">
    import RowDetail from "../components/RowDetail.svelte";
    import { getAdminContext } from "../lib/admin-context.js";
    import { useRoute } from "@dvcol/svelte-simple-router";
    import type { Row, Value, DibsError } from "@bearcove/dibs-admin/types";

    const ctx = getAdminContext();
    const routeState = $derived(useRoute());
    const params = $derived(routeState.location?.params as { table?: string; pk?: string } | undefined);
    const tableName = $derived(params?.table ?? "");
    const pk = $derived(params?.pk ?? "");

    let row = $state<Row | null>(null);
    let loading = $state(true);
    let error = $state<string | null>(null);
    let deleting = $state(false);

    // Derived
    let currentTable = $derived(ctx.schema?.tables.find((t) => t.name === tableName) ?? null);
    let columns = $derived(currentTable?.columns ?? []);

    // Load row when params change
    $effect(() => {
        if (ctx.schema && tableName && pk) {
            loadRow();
        }
    });

    async function loadRow() {
        if (!currentTable) return;

        const pkCol = currentTable.columns.find((c) => c.primary_key);
        if (!pkCol) return;

        loading = true;
        error = null;

        try {
            const pkValue = parsePkValue(pk, pkCol.sql_type);
            const result = await ctx.client.get({
                table: tableName,
                pk: pkValue,
            });
            if (result.ok && result.value) {
                row = result.value;
            } else {
                row = null;
                ctx.navigateToTable(tableName);
            }
        } catch (e) {
            console.error("Failed to load row:", e);
            row = null;
            ctx.navigateToTable(tableName);
        } finally {
            loading = false;
        }
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

    function formatPkValue(value: Value): string {
        if (value.tag === "Null") return "";
        if (typeof value.value === "bigint") return value.value.toString();
        return String(value.value);
    }

    function formatError(err: DibsError): string {
        if (err.tag === "MigrationFailed") {
            return `${err.tag}: ${err.value.message}`;
        }
        return `${err.tag}: ${err.value}`;
    }

    function getPrimaryKeyValue(): Value | null {
        if (!currentTable || !row) return null;
        const pkCol = currentTable.columns.find((c) => c.primary_key);
        if (!pkCol) return null;
        const field = row.fields.find((f) => f.name === pkCol.name);
        return field?.value ?? null;
    }

    async function saveField(fieldName: string, newValue: Value) {
        if (!row) {
            throw new Error("No row being edited");
        }

        const pkValue = getPrimaryKeyValue();
        if (!pkValue) {
            throw new Error("Could not determine primary key");
        }

        const updateData: Row = {
            fields: [{ name: fieldName, value: newValue }],
        };

        const result = await ctx.client.update({
            table: tableName,
            pk: pkValue,
            data: updateData,
        });

        if (!result.ok) {
            throw new Error(formatError(result.error));
        }

        // Update local row
        row = {
            fields: row.fields.map((f) =>
                f.name === fieldName ? { name: fieldName, value: newValue } : f,
            ),
        };
    }

    async function deleteRow() {
        if (!row) return;

        const pkValue = getPrimaryKeyValue();
        if (!pkValue) {
            error = "Could not determine primary key";
            return;
        }

        deleting = true;
        error = null;

        try {
            const result = await ctx.client.delete({
                table: tableName,
                pk: pkValue,
            });
            if (!result.ok) {
                error = formatError(result.error);
                return;
            }
            ctx.navigateToTable(tableName);
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            deleting = false;
        }
    }

    function closeEditor() {
        ctx.navigateToTable(tableName);
    }

    function handleRelatedNavigate(relatedTable: string, pkValue: Value) {
        const pkStr = formatPkValue(pkValue);
        ctx.addBreadcrumb({
            table: relatedTable,
            label: `${relatedTable} #${pkStr}`,
            pkValue,
        });
        ctx.navigateToRow(relatedTable, pkStr);
    }
</script>

{#if loading}
    <div class="loading-state">Loading...</div>
{:else if row && currentTable && ctx.schema}
    <RowDetail
        {columns}
        {row}
        table={currentTable}
        schema={ctx.schema}
        client={ctx.client}
        {tableName}
        config={ctx.config}
        onFieldSave={saveField}
        onDelete={deleteRow}
        onClose={closeEditor}
        {deleting}
        onNavigate={handleRelatedNavigate}
    />
{:else}
    <div class="error-state">Row not found</div>
{/if}

<style>
    .loading-state,
    .error-state {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: 2rem;
        color: var(--muted-foreground);
    }

    .error-state {
        color: var(--destructive);
    }
</style>
