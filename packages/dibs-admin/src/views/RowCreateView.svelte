<script lang="ts">
    import RowEditor from "../components/RowEditor.svelte";
    import { getAdminContext } from "../lib/admin-context.js";
    import { useRoute, useNavigate } from "@dvcol/svelte-simple-router/router";
    import type { Row, DibsError } from "@bearcove/dibs-admin/types";

    const ctx = getAdminContext();
    const routeState = useRoute();
    const navigate = useNavigate();

    // Get table name from route params
    const tableName = $derived((routeState.route?.params as { table?: string })?.table ?? "");

    let saving = $state(false);
    let error = $state<string | null>(null);

    // Derived
    let currentTable = $derived(ctx.schema?.tables.find((t) => t.name === tableName) ?? null);
    let columns = $derived(currentTable?.columns ?? []);

    function formatError(err: DibsError): string {
        if (err.tag === "MigrationFailed") {
            return `${err.tag}: ${err.value.message}`;
        }
        return `${err.tag}: ${err.value}`;
    }

    async function saveRow(data: Row) {
        saving = true;
        error = null;

        try {
            const result = await ctx.client.create({
                table: tableName,
                data,
            });
            if (!result.ok) {
                error = formatError(result.error);
                return;
            }
            navigate.push({ path: tableName });
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            saving = false;
        }
    }

    function closeEditor() {
        navigate.push({ path: tableName });
    }
</script>

{#if currentTable && ctx.schema}
    <RowEditor
        {columns}
        row={null}
        onSave={saveRow}
        onClose={closeEditor}
        {saving}
        table={currentTable}
        schema={ctx.schema}
        client={ctx.client}
        fullscreen={true}
        {tableName}
    />
{:else}
    <div class="error-state">Table not found</div>
{/if}

<style>
    .error-state {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: 2rem;
        color: var(--destructive);
    }
</style>
