<script lang="ts">
    import { ArrowSquareOut } from "phosphor-svelte";
    import type { Row, TableInfo, Value } from "../types.js";
    import { formatValueForDisplay, getDisplayColumn } from "../lib/fk-utils.js";

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
        const pkCol = table.columns.find(c => c.primary_key);

        // Prioritize: PK, display column, then first few text columns
        const priority = [pkCol?.name, displayCol?.name].filter(Boolean) as string[];
        const shown = new Set<string>();
        const result: { name: string; value: string }[] = [];

        // Add priority columns first
        for (const name of priority) {
            const field = row.fields.find(f => f.name === name);
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

<div class="bg-neutral-900 border border-neutral-800 p-3 min-w-[200px] max-w-[300px] shadow-xl">
    {#if loading}
        <div class="text-neutral-500 text-xs">Loading...</div>
    {:else if error}
        <div class="text-red-400 text-xs">{error}</div>
    {:else if row && table}
        <div class="text-xs text-neutral-500 mb-2 flex items-center gap-1">
            <ArrowSquareOut size={12} />
            {table.name}
        </div>
        <div class="space-y-1">
            {#each previewFields as field}
                <div class="flex gap-2 text-sm">
                    <span class="text-neutral-500 shrink-0">{field.name}:</span>
                    <span class="text-white truncate">{field.value}</span>
                </div>
            {/each}
        </div>
    {:else}
        <div class="text-neutral-500 text-xs">No data</div>
    {/if}
</div>
