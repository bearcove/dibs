<script lang="ts">
    import { DibsAdmin, type DibsAdminConfig } from "@bearcove/dibs-admin";
    import { connect, getClient } from "./lib/roam";
    import "./app.css";

    // Connection state
    let connected = $state(false);
    let connecting = $state(false);
    let error = $state<string | null>(null);

    // Database URL - matches my-app-db/.env default
    const DATABASE_URL = "postgres://localhost/dibs_test";

    // Admin configuration for ecommerce schema
    const config: DibsAdminConfig = {
        dashboard: {
            title: "Ecommerce Admin",
            tiles: [
                { type: "latest", table: "product", title: "Recent Products", limit: 5 },
                { type: "latest", table: "product_variant", title: "Recent Variants", limit: 5 },
                { type: "count", table: "product", title: "Total Products", icon: "package" },
                { type: "count", table: "product_variant", title: "Total Variants", icon: "layers" },
                { type: "count", table: "variant_price", title: "Total Prices", icon: "coins" },
                {
                    type: "links",
                    title: "Quick Links",
                    links: [
                        { label: "All Products", table: "product" },
                        { label: "All Variants", table: "product_variant" },
                        { label: "Translations", table: "product_translation" },
                    ],
                },
            ],
        },

        tables: {
            product: {
                label: "Products",
                list: {
                    columns: ["id", "handle", "status", "active", "created_at"],
                    defaultSort: { field: "created_at", direction: "desc" },
                },
                detail: {
                    fields: [
                        { title: "Basic Info", fields: ["handle", "status", "active"] },
                        { title: "Metadata", fields: ["metadata"], collapsed: true },
                        { title: "Timestamps", fields: ["id", "created_at", "updated_at", "deleted_at"], collapsed: true },
                    ],
                    readOnly: ["id", "created_at", "updated_at"],
                },
                relations: [
                    { table: "product_variant", via: "product_id", label: "Variants", limit: 10 },
                    { table: "product_translation", via: "product_id", label: "Translations", limit: 10 },
                    { table: "product_source", via: "product_id", label: "Sources", limit: 5 },
                ],
            },

            product_variant: {
                label: "Variants",
                list: {
                    columns: ["id", "product_id", "sku", "title", "manage_inventory", "sort_order"],
                    defaultSort: { field: "created_at", direction: "desc" },
                },
                detail: {
                    fields: [
                        { title: "Basic Info", fields: ["sku", "title", "product_id"] },
                        { title: "Inventory", fields: ["manage_inventory", "allow_backorder", "sort_order"] },
                        { title: "Attributes", fields: ["attributes"], collapsed: true },
                        { title: "Timestamps", fields: ["id", "created_at", "updated_at", "deleted_at"], collapsed: true },
                    ],
                    readOnly: ["id", "created_at", "updated_at"],
                },
                relations: [
                    { table: "variant_price", via: "variant_id", label: "Prices", limit: 10 },
                ],
            },

            variant_price: {
                label: "Prices",
                list: {
                    columns: ["id", "variant_id", "currency_code", "amount", "region"],
                    defaultSort: { field: "created_at", direction: "desc" },
                },
                detail: {
                    fields: [
                        { title: "Price Info", fields: ["variant_id", "currency_code", "amount", "region"] },
                        { title: "Timestamps", fields: ["id", "created_at", "updated_at"], collapsed: true },
                    ],
                    readOnly: ["id", "created_at", "updated_at"],
                },
            },

            product_source: {
                label: "Sources",
                list: {
                    columns: ["id", "product_id", "vendor", "external_id", "last_synced_at"],
                    defaultSort: { field: "last_synced_at", direction: "desc" },
                },
                detail: {
                    fields: [
                        { title: "Source Info", fields: ["product_id", "vendor", "external_id"] },
                        { title: "Sync Info", fields: ["last_synced_at", "raw_data"], collapsed: true },
                    ],
                    readOnly: ["id"],
                },
            },

            product_translation: {
                label: "Translations",
                list: {
                    columns: ["id", "product_id", "locale", "title"],
                    defaultSort: { field: "locale", direction: "asc" },
                },
                detail: {
                    fields: [
                        { title: "Translation", fields: ["product_id", "locale", "title", "description"] },
                    ],
                    readOnly: ["id"],
                },
            },
        },

        defaults: {
            pageSize: 25,
            relationLimit: 10,
        },
    };

    async function handleConnect() {
        connecting = true;
        error = null;
        try {
            await connect();
            connected = true;
        } catch (e) {
            error = e instanceof Error ? e.message : String(e);
        } finally {
            connecting = false;
        }
    }

    // Auto-connect on mount
    $effect(() => {
        handleConnect();
    });
</script>

<svelte:head>
    <style>
        body {
            margin: 0;
        }
    </style>
</svelte:head>

<main class="min-h-screen flex flex-col bg-background text-foreground">
    <header class="px-6 py-4 border-b border-border flex items-center gap-3">
        <h1 class="text-sm font-medium uppercase tracking-widest text-muted-foreground">dibs admin</h1>
        {#if connected}
            <span class="w-2 h-2 bg-green-500"></span>
        {:else if connecting}
            <span class="w-2 h-2 bg-yellow-500 animate-pulse"></span>
        {:else if error}
            <span class="w-2 h-2 bg-red-500"></span>
        {/if}
    </header>

    {#if !connected && !connecting}
        <div class="flex-1 flex flex-col items-center justify-center gap-6 p-8">
            {#if error}
                <p class="text-destructive text-sm mb-4">
                    {error}
                </p>
            {/if}
            <button
                class="bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-40 disabled:cursor-not-allowed px-6 py-3 text-sm font-medium transition-colors rounded-md"
                onclick={handleConnect}
            >
                Retry connection
            </button>
        </div>
    {:else if connecting}
        <div class="flex-1 flex items-center justify-center text-muted-foreground">
            Connecting...
        </div>
    {:else}
        {@const client = getClient()}
        {#if client}
            <div class="flex-1 min-h-0">
                <DibsAdmin {client} databaseUrl={DATABASE_URL} {config} />
            </div>
        {/if}
    {/if}
</main>
