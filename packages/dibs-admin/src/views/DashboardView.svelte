<script lang="ts">
    import Dashboard from "../components/Dashboard.svelte";
    import { getAdminContext } from "../lib/admin-context.js";

    const ctx = getAdminContext();
</script>

{#if ctx.config?.dashboard && ctx.schema}
    <Dashboard
        config={ctx.config}
        schema={ctx.schema}
        client={ctx.client}
        onSelectTable={ctx.navigateToTable}
    />
{:else}
    <div class="no-dashboard">No dashboard configured</div>
{/if}

<style>
    .no-dashboard {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: 2rem;
        color: var(--muted-foreground);
    }
</style>
