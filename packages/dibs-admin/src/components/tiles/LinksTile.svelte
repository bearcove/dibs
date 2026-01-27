<script lang="ts">
    import { ArrowRight } from "phosphor-svelte";
    import type { QuickLinksTile } from "../../types/config";
    import { Card } from "../../lib/ui/index";

    interface Props {
        config: QuickLinksTile;
        onSelectTable: (tableName: string) => void;
    }

    let { config, onSelectTable }: Props = $props();

    let title = $derived(config.title ?? "Quick Links");
</script>

<Card.Root>
    <Card.Header class="tile-header">
        <Card.Title class="tile-title">{title}</Card.Title>
    </Card.Header>
    <Card.Content class="tile-content">
        <ul class="links-list">
            {#each config.links as link}
                <li>
                    <button class="link-button" onclick={() => onSelectTable(link.table)}>
                        <span>{link.label}</span>
                        <ArrowRight size={14} class="link-arrow" />
                    </button>
                </li>
            {/each}
        </ul>
    </Card.Content>
</Card.Root>

<style>
    :global(.tile-header) {
        padding-bottom: 0.75rem;
    }

    :global(.tile-title) {
        font-size: 0.875rem;
        font-weight: 500;
    }

    :global(.tile-content) {
        padding-top: 0;
    }

    .links-list {
        list-style: none;
        padding: 0;
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .link-button {
        width: 100%;
        display: flex;
        align-items: center;
        justify-content: space-between;
        font-size: 0.875rem;
        text-align: left;
        background: none;
        border: none;
        padding: 0;
        cursor: pointer;
        color: var(--foreground);
        transition: color 0.15s;
    }

    .link-button:hover {
        color: var(--primary);
    }

    :global(.link-arrow) {
        color: var(--muted-foreground);
    }
</style>
