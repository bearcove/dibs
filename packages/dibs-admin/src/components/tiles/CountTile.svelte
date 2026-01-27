<script lang="ts">
    import type { CountTile } from "../../types/config.js";
    import type { SchemaInfo, SquelClient } from "../../types.js";
    import { filterConfigsToFilters } from "../../lib/config.js";
    import { Card } from "../../lib/components/ui/index.js";
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
                error = result.error.value;
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

<Card.Root
    class="cursor-pointer hover:bg-accent/50 transition-colors"
    onclick={() => onSelectTable(config.table)}
>
    <Card.Content class="p-6">
        <div class="flex items-center justify-between">
            <div>
                <p class="text-sm text-muted-foreground">{title}</p>
                {#if loading}
                    <p class="text-2xl font-semibold text-muted-foreground">—</p>
                {:else if error}
                    <p class="text-sm text-destructive">{error}</p>
                {:else if count !== null}
                    <p class="text-2xl font-semibold">{formatCount(count)}</p>
                {/if}
            </div>
            <div class="p-3 rounded-full bg-muted">
                <DynamicIcon name={icon} size={24} class="text-muted-foreground" />
            </div>
        </div>
    </Card.Content>
</Card.Root>
