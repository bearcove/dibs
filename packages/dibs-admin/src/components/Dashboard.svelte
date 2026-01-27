<script lang="ts">
    import type {
        DibsAdminConfig,
        DashboardTile,
        LatestRecordsTile,
        CountTile,
        QuickLinksTile,
    } from "../types/config";
    import type { SchemaInfo, SquelClient } from "../types";
    import LatestTile from "./tiles/LatestTile.svelte";
    import CountTileComponent from "./tiles/CountTile.svelte";
    import LinksTile from "./tiles/LinksTile.svelte";

    interface Props {
        config: DibsAdminConfig;
        schema: SchemaInfo;
        client: SquelClient;
        onSelectTable: (tableName: string) => void;
    }

    let { config, schema, client, onSelectTable }: Props = $props();

    let dashboardConfig = $derived(config.dashboard);
    let tiles = $derived(dashboardConfig?.tiles ?? []);
    let title = $derived(dashboardConfig?.title ?? "Dashboard");
</script>

<section class="dashboard">
    <h1 class="dashboard-title">{title}</h1>

    <div class="tiles-grid">
        {#each tiles as tile}
            {#if tile.type === "latest"}
                <LatestTile config={tile} {schema} {client} {onSelectTable} />
            {:else if tile.type === "count"}
                <CountTileComponent config={tile} {schema} {client} {onSelectTable} />
            {:else if tile.type === "links"}
                <LinksTile config={tile} {onSelectTable} />
            {:else if tile.type === "custom"}
                {@const CustomComponent = tile.component}
                <CustomComponent />
            {/if}
        {/each}
    </div>

    {#if tiles.length === 0}
        <div class="empty-state">No dashboard tiles configured</div>
    {/if}
</section>

<style>
    .dashboard {
        padding: 1.5rem;
        overflow: auto;
        display: flex;
        flex-direction: column;
        max-height: 100vh;
    }

    @media (min-width: 768px) {
        .dashboard {
            padding: 2rem;
        }
    }

    .dashboard-title {
        font-size: 1.5rem;
        font-weight: 600;
        color: var(--foreground);
        margin-top: 0;
        margin-bottom: 2rem;
    }

    .tiles-grid {
        display: grid;
        grid-template-columns: 1fr;
        gap: 1.5rem;
    }

    @media (min-width: 768px) {
        .tiles-grid {
            grid-template-columns: repeat(2, 1fr);
        }
    }

    @media (min-width: 1024px) {
        .tiles-grid {
            grid-template-columns: repeat(3, 1fr);
        }
    }

    .empty-state {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        color: color-mix(in oklch, var(--muted-foreground) 60%, transparent);
    }
</style>
