<script lang="ts">
    import { CaretRight } from "phosphor-svelte";
    import type { BreadcrumbEntry } from "../lib/fk-utils";

    interface Props {
        entries: BreadcrumbEntry[];
        onNavigate: (index: number) => void;
    }

    let { entries, onNavigate }: Props = $props();
</script>

{#if entries.length > 1}
    <nav class="breadcrumb">
        {#each entries as entry, i}
            {#if i > 0}
                <CaretRight size={12} class="separator" />
            {/if}
            {#if i < entries.length - 1}
                <button class="breadcrumb-link" onclick={() => onNavigate(i)}>
                    {entry.label}
                </button>
            {:else}
                <span class="breadcrumb-current">{entry.label}</span>
            {/if}
        {/each}
    </nav>
{/if}

<style>
    .breadcrumb {
        display: flex;
        align-items: center;
        gap: 0.25rem;
        font-size: 0.875rem;
        margin-bottom: 1rem;
        color: var(--muted-foreground);
    }

    .breadcrumb :global(.separator) {
        color: color-mix(in oklch, var(--muted-foreground) 40%, transparent);
    }

    .breadcrumb-link {
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        color: inherit;
        cursor: pointer;
        transition: color 0.15s;
    }

    .breadcrumb-link:hover {
        color: var(--foreground);
    }

    .breadcrumb-current {
        color: var(--foreground);
    }
</style>
