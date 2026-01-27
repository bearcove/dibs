<script lang="ts">
    import type {
        DibsAdminConfig,
        DashboardTile,
        LatestRecordsTile,
        CountTile,
        QuickLinksTile,
    } from "../types/config.js";
    import type { SchemaInfo, SquelClient } from "../types.js";
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

<section class="p-6 md:p-8 overflow-auto flex flex-col max-h-screen">
    <h1 class="text-2xl font-semibold text-foreground mb-8">{title}</h1>

    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
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
        <div class="flex-1 flex items-center justify-center text-muted-foreground/60">
            No dashboard tiles configured
        </div>
    {/if}
</section>
