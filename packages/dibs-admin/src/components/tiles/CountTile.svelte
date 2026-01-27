<script lang="ts">
    import type { CountTile } from "../../types/config";
    import type { SchemaInfo, SquelClient } from "../../types";
    import { filterConfigsToFilters } from "../../lib/config";
    import { Card } from "../../lib/ui/index";
    import DynamicIcon from "../DynamicIcon.svelte";

    interface Props {
        config: CountTile;
        schema: SchemaInfo;
        client: SquelClient;

        onSelectTable: (tableName: string) => void;
    }

    let { config, schema, client, onSelectTable }: Props = $props();

    let count = $state<bigint | null>(null);
    let loading = $state(true);
    let error = $state<string | null>(null);

    let tableInfo = $derived(schema.tables.find((t) => t.name === config.table));
    let title = $derived(config.title ?? config.table);
    let icon = $derived(config.icon ?? tableInfo?.icon ?? "hash");

    $effect(() => {
        loadCount();
    });

    async function loadCount() {
        if (!config.table) return;

        loading = true;
        error = null;

        try {
            // Convert filter config to internal filter format
            const filters = config.filter ? filterConfigsToFilters(config.filter) : [];

            const result = await client.list({
                table: config.table,
                filters,
                sort: [],
                limit: 1, // Request 1 row to trigger proper total computation
                offset: null,
                select: [],
            });

            if (result.ok) {
                count = result.value.total ?? BigInt(result.value.rows.length);
            } else {
                error =
                    result.error.tag === "MigrationFailed"
                        ? result.error.value.message
                        : result.error.value;
            }
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            loading = false;
        }
    }

    function formatCount(n: bigint): string {
        if (n >= 1_000_000n) {
            return `${(Number(n) / 1_000_000).toFixed(1)}M`;
        }
        if (n >= 1_000n) {
            return `${(Number(n) / 1_000).toFixed(1)}K`;
        }
        return n.toString();
    }
</script>

<button type="button" class="tile-button" onclick={() => onSelectTable(config.table)}>
    <Card.Root>
        <Card.Content class="tile-content">
            <div class="tile-inner">
                <div>
                    <p class="tile-title">{title}</p>
                    {#if loading}
                        <p class="tile-count muted">—</p>
                    {:else if error}
                        <p class="tile-error">{error}</p>
                    {:else if count !== null}
                        <p class="tile-count">{formatCount(count)}</p>
                    {/if}
                </div>
                <div class="tile-icon-wrapper">
                    <DynamicIcon name={icon} size={24} class="tile-icon" />
                </div>
            </div>
        </Card.Content>
    </Card.Root>
</button>

<style>
    .tile-button {
        display: block;
        width: 100%;
        padding: 0;
        border: none;
        background: transparent;
        cursor: pointer;
        text-align: left;
    }

    .tile-button:hover :global(.card) {
        background-color: oklch(from var(--accent) l c h / 0.5);
    }

    :global(.tile-content) {
        padding: 1.5rem;
    }

    .tile-inner {
        display: flex;
        align-items: center;
        justify-content: space-between;
    }

    .tile-title {
        font-size: 0.875rem;
        color: var(--muted-foreground);
        margin: 0;
    }

    .tile-count {
        font-size: 1.5rem;
        font-weight: 600;
        margin: 0;
        color: var(--foreground);
    }

    .tile-count.muted {
        color: var(--muted-foreground);
    }

    .tile-error {
        font-size: 0.875rem;
        color: var(--destructive);
        margin: 0;
    }

    .tile-icon-wrapper {
        padding: 0.75rem;
        border-radius: 9999px;
        background-color: var(--muted);
    }

    :global(.tile-icon) {
        color: var(--muted-foreground);
    }
</style>
