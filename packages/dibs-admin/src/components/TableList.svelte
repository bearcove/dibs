<script lang="ts">
    import type { TableInfo } from "../types.js";
    import DynamicIcon from "./DynamicIcon.svelte";

    interface Props {
        tables: TableInfo[];
        selected: string | null;
        onSelect: (tableName: string) => void;
    }

    let { tables, selected, onSelect }: Props = $props();
</script>

<aside class="bg-sidebar p-6 overflow-y-auto border-r border-sidebar-border">
    <h2 class="text-xs font-medium text-sidebar-foreground/60 uppercase tracking-widest mb-6">Tables</h2>
    <ul class="space-y-1">
        {#each tables as table}
            <li>
                <button
                    class="w-full text-left px-3 py-2 text-sm rounded-md transition-all duration-150 flex items-center gap-2 {selected ===
                    table.name
                        ? 'text-sidebar-primary-foreground bg-sidebar-primary font-medium'
                        : 'text-sidebar-foreground/70 hover:text-sidebar-foreground hover:bg-sidebar-accent'}"
                    onclick={() => onSelect(table.name)}
                >
                    <DynamicIcon name={table.icon ?? "table"} size={16} class="shrink-0 opacity-70" />
                    <span class="truncate">{table.name}</span>
                </button>
            </li>
        {/each}
    </ul>
</aside>
