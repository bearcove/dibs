<script lang="ts">
    import ArrowSquareOutIcon from "phosphor-svelte/lib/ArrowSquareOutIcon";
    import type { Row, TableInfo, Value } from "@bearcove/dibs-admin/types";
    import { formatValueForDisplay, getDisplayColumn } from "@bearcove/dibs-admin/lib/fk-utils";

    interface Props {
        row: Row | null;
        table: TableInfo | null;
        loading: boolean;
        error: string | null;
    }

    let { row, table, loading, error }: Props = $props();

    // Get the most important fields to show (PK + display column + a few more)
    function getPreviewFields(): { name: string; value: string }[] {
        if (!row || !table) return [];

        const displayCol = getDisplayColumn(table);
        const pkCol = table.columns.find((c) => c.primary_key);

        // Prioritize: PK, display column, then first few text columns
        const priority = [pkCol?.name, displayCol?.name].filter(Boolean) as string[];
        const shown = new Set<string>();
        const result: { name: string; value: string }[] = [];

        // Add priority columns first
        for (const name of priority) {
            const field = row.fields.find((f) => f.name === name);
            if (field && !shown.has(name)) {
                result.push({ name, value: formatValueForDisplay(field.value) });
                shown.add(name);
            }
        }

        // Add a few more columns (up to 5 total)
        for (const field of row.fields) {
            if (result.length >= 5) break;
            if (shown.has(field.name)) continue;
            if (field.value.tag === "Bytes") continue; // Skip binary
            result.push({ name: field.name, value: formatValueForDisplay(field.value) });
            shown.add(field.name);
        }

        return result;
    }

    let previewFields = $derived(getPreviewFields());
</script>

<div class="fk-preview">
    {#if loading}
        <div class="status-message">Loading...</div>
    {:else if error}
        <div class="status-message error">{error}</div>
    {:else if row && table}
        <div class="header">
            <ArrowSquareOutIcon size={12} class="icon" />
            <span class="table-name">{table.name}</span>
        </div>
        <div class="fields">
            {#each previewFields as field}
                <div class="field-row">
                    <span class="field-name">{field.name}</span>
                    <span class="field-value">{field.value}</span>
                </div>
            {/each}
        </div>
    {:else}
        <div class="status-message">No data</div>
    {/if}
</div>

<style>
    .fk-preview {
        background-color: var(--popover);
        border: 1px solid var(--border);
        min-width: 220px;
        max-width: 320px;
        box-shadow:
            0 10px 15px -3px rgb(0 0 0 / 0.1),
            0 4px 6px -4px rgb(0 0 0 / 0.1);
        border-radius: var(--radius-lg);
        overflow: hidden;
    }

    .status-message {
        color: var(--muted-foreground);
        font-size: 0.75rem;
        padding: 0.75rem;
    }

    .status-message.error {
        color: var(--destructive);
    }

    .header {
        background-color: color-mix(in oklch, var(--accent) 50%, transparent);
        padding: 0.5rem 0.75rem;
        border-bottom: 1px solid var(--border);
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .header :global(.icon) {
        color: var(--primary);
    }

    .table-name {
        font-size: 0.75rem;
        font-weight: 500;
        color: var(--accent-foreground);
    }

    .fields {
        padding: 0.75rem;
        display: flex;
        flex-direction: column;
        gap: 0.375rem;
    }

    .field-row {
        display: flex;
        gap: 0.5rem;
        font-size: 0.875rem;
    }

    .field-name {
        color: var(--muted-foreground);
        flex-shrink: 0;
        min-width: 70px;
    }

    .field-value {
        color: var(--popover-foreground);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-weight: 500;
    }
</style>
