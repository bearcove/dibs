<script lang="ts">
    import { untrack } from "svelte";
    import { RouterView, RouteView, useRouter, useRoute } from "@dvcol/svelte-simple-router";
    import type { PartialRoute } from "@dvcol/svelte-simple-router";
    import type { SquelServiceCaller, SchemaInfo, Row } from "@bearcove/dibs-admin/types";
    import type { DibsAdminConfig } from "@bearcove/dibs-admin/types/config";
    import TableList from "./components/TableList.svelte";
    import { Tooltip } from "@bearcove/dibs-admin/lib/ui";
    import { isTableHidden, hasDashboard } from "@bearcove/dibs-admin/lib/config";
    import { schemaCache } from "./lib/schema-cache.js";
    import { setAdminContext, type BreadcrumbEntry } from "./lib/admin-context.js";
    import "@bearcove/dibs-admin/styles/tokens.css";

    // Views
    import DashboardView from "./views/DashboardView.svelte";
    import TableListView from "./views/TableListView.svelte";
    import RowDetailView from "./views/RowDetailView.svelte";
    import RowCreateView from "./views/RowCreateView.svelte";

    interface Props {
        client: SquelServiceCaller;
        config?: DibsAdminConfig;
    }

    let { client, config }: Props = $props();

    // Get router for navigation
    const router = useRouter();
    const routeState = $derived(useRoute());

    // Derive base path from the matched route (e.g., "/admin/*" -> "/admin")
    const basePath = $derived(routeState.route?.path?.replace(/\/?\*$/, "") ?? "");

    // Schema state
    let schema = $state<SchemaInfo | null>(schemaCache.get(client) ?? null);
    let loading = $state(false);
    let error = $state<string | null>(null);

    // Shared state
    let fkLookup = $state<Map<string, Map<string, Row>>>(new Map());
    let timeMode = $state<"relative" | "absolute">("relative");
    let breadcrumbs = $state<BreadcrumbEntry[]>([]);

    // Prevent double-loading
    let schemaLoaded = false;

    // Current location params
    const locationParams = $derived(routeState.location?.params as { table?: string; pk?: string } | undefined);
    const selectedTable = $derived(locationParams?.table ?? null);
    const showDashboard = $derived(!locationParams?.table);

    // Filter hidden tables
    let visibleTables = $derived(
        schema?.tables.filter((t) => !isTableHidden(config, t.name)) ?? [],
    );

    // Navigation functions using the derived base path
    function navigateToDashboard() {
        breadcrumbs = [];
        router?.push({ path: basePath || "/" });
    }

    function navigateToTable(table: string) {
        breadcrumbs = [{ table, label: table }];
        router?.push({ path: `${basePath}/${table}` });
    }

    function navigateToRow(table: string, pk: string) {
        router?.push({ path: `${basePath}/${table}/${pk}` });
    }

    function navigateToNewRow(table: string) {
        router?.push({ path: `${basePath}/${table}/new` });
    }

    // Context
    setAdminContext({
        client,
        config,
        get schema() {
            return schema;
        },
        navigateToDashboard,
        navigateToTable,
        navigateToRow,
        navigateToNewRow,
        get fkLookup() {
            return fkLookup;
        },
        setFkLookup: (lookup) => {
            fkLookup = lookup;
        },
        get timeMode() {
            return timeMode;
        },
        setTimeMode: (mode) => {
            timeMode = mode;
        },
        get breadcrumbs() {
            return breadcrumbs;
        },
        setBreadcrumbs: (entries) => {
            breadcrumbs = entries;
        },
        addBreadcrumb: (entry) => {
            breadcrumbs = [...breadcrumbs, entry];
        },
    });

    // Load schema on mount
    $effect(() => {
        untrack(() => loadSchema());
    });

    async function loadSchema() {
        if (schemaLoaded) return;
        schemaLoaded = true;

        const cached = schemaCache.get(client);
        if (cached) {
            schema = cached;
        } else {
            loading = true;
            error = null;
            try {
                schema = await client.schema();
                schemaCache.set(client, schema);
            } catch (e) {
                error = e instanceof Error ? e.message : String(e);
            } finally {
                loading = false;
            }
            if (!schema) return;
        }

        // Initialize breadcrumbs if we have a table from the route
        if (selectedTable) {
            breadcrumbs = [{ table: selectedTable, label: selectedTable }];
        }

        // If no table selected and no dashboard, navigate to first table
        if (!selectedTable && !hasDashboard(config) && schema.tables.length > 0) {
            const firstVisible = schema.tables.find((t) => !isTableHidden(config, t.name));
            if (firstVisible) {
                navigateToTable(firstVisible.name);
            }
        }
    }

    function selectTable(tableName: string) {
        navigateToTable(tableName);
    }

    function goToDashboard() {
        navigateToDashboard();
    }

    // Typed route definitions for RouteView
    const dashboardRoute: PartialRoute = $derived({
        name: "dibs-dashboard",
        path: basePath || "/",
        component: DashboardView,
    });
    const tableNewRoute: PartialRoute = $derived({
        name: "dibs-table-new",
        path: `${basePath}/:table/new`,
        component: RowCreateView,
    });
    const tableRowRoute: PartialRoute = $derived({
        name: "dibs-table-row",
        path: `${basePath}/:table/:pk`,
        component: RowDetailView,
    });
    const tableRoute: PartialRoute = $derived({
        name: "dibs-table",
        path: `${basePath}/:table`,
        component: TableListView,
    });
</script>

<Tooltip.Provider>
    <div class="admin-root">
        {#if loading && !schema}
            <div class="loading-state">Loading schema...</div>
        {:else if schema}
            <div class="admin-layout">
                <TableList
                    tables={visibleTables}
                    selected={selectedTable}
                    onSelect={selectTable}
                    {config}
                    showDashboardButton={hasDashboard(config)}
                    onDashboard={goToDashboard}
                    dashboardActive={showDashboard}
                    {timeMode}
                    onTimeModeChange={(mode) => (timeMode = mode)}
                />

                <main class="admin-content">
                    <RouterView>
                        <!-- Register child routes via RouteView -->
                        <RouteView {...{ route: dashboardRoute } as any} />
                        <RouteView {...{ route: tableNewRoute } as any} />
                        <RouteView {...{ route: tableRowRoute } as any} />
                        <RouteView {...{ route: tableRoute } as any} />
                    </RouterView>
                </main>
            </div>
        {:else if error}
            <p class="error-message standalone">{error}</p>
        {/if}
    </div>
</Tooltip.Provider>

<style>
    .admin-root {
        height: 100%;
        min-height: 100vh;
        background-color: var(--background);
        color: var(--foreground);
    }

    .loading-state {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: 2rem;
        color: var(--muted-foreground);
    }

    .admin-layout {
        display: grid;
        grid-template-columns: 280px 1fr;
        min-height: 100vh;
    }

    .admin-content {
        overflow: auto;
        display: flex;
        flex-direction: column;
        max-height: 100vh;
    }

    .error-message {
        color: var(--destructive);
        margin-bottom: 1.5rem;
        font-size: 0.875rem;
    }

    .error-message.standalone {
        padding: 2rem;
        margin-bottom: 0;
    }
</style>
