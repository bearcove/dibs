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

<div class="bg-popover border border-border min-w-[220px] max-w-[320px] shadow-xl rounded-lg overflow-hidden">
    {#if loading}
        <div class="text-muted-foreground text-xs p-3">Loading...</div>
    {:else if error}
        <div class="text-destructive text-xs p-3">{error}</div>
    {:else if row && table}
        <div class="bg-accent/50 px-3 py-2 border-b border-border flex items-center gap-2">
            <ArrowSquareOut size={12} class="text-primary" />
            <span class="text-xs font-medium text-accent-foreground">{table.name}</span>
        </div>
        <div class="p-3 space-y-1.5">
            {#each previewFields as field, i}
                <div class="flex gap-2 text-sm">
                    <span class="text-muted-foreground shrink-0 min-w-[70px]">{field.name}</span>
                    <span class="text-popover-foreground truncate font-medium">{field.value}</span>
                </div>
            {/each}
        </div>
    {:else}
        <div class="text-muted-foreground text-xs p-3">No data</div>
    {/if}
</div>
