<script lang="ts">
    import { CaretRight } from "phosphor-svelte";
    import type { BreadcrumbEntry } from "../lib/fk-utils.js";

    interface Props {
        entries: BreadcrumbEntry[];
        onNavigate: (index: number) => void;
    }

    let { entries, onNavigate }: Props = $props();
</script>

{#if entries.length > 1}
    <nav class="flex items-center gap-1 text-sm mb-4 text-neutral-500">
        {#each entries as entry, i}
            {#if i > 0}
                <CaretRight size={12} class="text-neutral-700" />
            {/if}
            {#if i < entries.length - 1}
                <button
                    class="hover:text-white transition-colors"
                    onclick={() => onNavigate(i)}
                >
                    {entry.label}
                </button>
            {:else}
                <span class="text-white">{entry.label}</span>
            {/if}
        {/each}
    </nav>
{/if}
