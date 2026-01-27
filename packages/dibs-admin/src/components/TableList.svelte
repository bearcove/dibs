<script lang="ts">
    import CaretDownIcon from "phosphor-svelte/lib/CaretDownIcon";
    import CaretRightIcon from "phosphor-svelte/lib/CaretRightIcon";
    import ClockIcon from "phosphor-svelte/lib/ClockIcon";
    import HouseIcon from "phosphor-svelte/lib/HouseIcon";
    import MagnifyingGlassIcon from "phosphor-svelte/lib/MagnifyingGlassIcon";
    import type { TableInfo } from "@bearcove/dibs-admin/types";
    import type { DibsAdminConfig, TableGroupConfig } from "@bearcove/dibs-admin/types/config";
    import { getTableLabel } from "@bearcove/dibs-admin/lib/config";
    import DynamicIcon from "./DynamicIcon.svelte";

    interface Props {
        tables: TableInfo[];
        selected: string | null;
        onSelect: (tableName: string) => void;
        config?: DibsAdminConfig;
        showDashboardButton?: boolean;
        onDashboard?: () => void;
        dashboardActive?: boolean;
        timeMode?: "relative" | "absolute";
        onTimeModeChange?: (mode: "relative" | "absolute") => void;
    }

    let {
        tables,
        selected,
        onSelect,
        config,
        showDashboardButton = false,
        onDashboard,
        dashboardActive = false,
        timeMode = "relative",
        onTimeModeChange,
    }: Props = $props();

    let filterText = $state("");

    // Track collapsed state for each group
    let collapsedGroups = $state<Record<string, boolean>>({});

    // Initialize collapsed state from config
    $effect(() => {
        if (config?.groups) {
            const initial: Record<string, boolean> = {};
            for (const group of config.groups) {
                if (group.collapsed && !(group.name in collapsedGroups)) {
                    initial[group.name] = true;
                }
            }
            if (Object.keys(initial).length > 0) {
                collapsedGroups = { ...collapsedGroups, ...initial };
            }
        }
    });

    function toggleGroup(groupName: string) {
        collapsedGroups = {
            ...collapsedGroups,
            [groupName]: !collapsedGroups[groupName],
        };
    }

    // Build a lookup of table name -> TableInfo
    const tableMap = $derived(new Map(tables.map((t) => [t.name, t])));

    // Filter tables based on search
    const filteredTableNames = $derived(
        filterText.trim() === ""
            ? new Set(tables.map((t) => t.name))
            : new Set(
                  tables
                      .filter((t) => {
                          const label = getTableLabel(config, t.name).toLowerCase();
                          const name = t.name.toLowerCase();
                          const query = filterText.toLowerCase();
                          return label.includes(query) || name.includes(query);
                      })
                      .map((t) => t.name),
              ),
    );

    // Compute grouped tables and ungrouped tables
    const groupedDisplay = $derived.by(() => {
        const groups = config?.groups ?? [];
        const groupedTableNames = new Set<string>();

        // Collect all tables that are in groups
        for (const group of groups) {
            for (const tableName of group.tables) {
                groupedTableNames.add(tableName);
            }
        }

        // Build display groups with filtered tables
        const displayGroups: { group: TableGroupConfig; tables: TableInfo[] }[] = [];
        for (const group of groups) {
            const groupTables: TableInfo[] = [];
            for (const tableName of group.tables) {
                if (filteredTableNames.has(tableName)) {
                    const tableInfo = tableMap.get(tableName);
                    if (tableInfo) {
                        groupTables.push(tableInfo);
                    }
                }
            }
            if (groupTables.length > 0) {
                displayGroups.push({ group, tables: groupTables });
            }
        }

        // Find ungrouped tables
        const ungroupedTables: TableInfo[] = [];
        for (const table of tables) {
            if (!groupedTableNames.has(table.name) && filteredTableNames.has(table.name)) {
                ungroupedTables.push(table);
            }
        }

        return { groups: displayGroups, ungrouped: ungroupedTables };
    });

    function toggleTimeMode() {
        const newMode = timeMode === "relative" ? "absolute" : "relative";
        onTimeModeChange?.(newMode);
    }
</script>

<aside class="sidebar">
    {#if showDashboardButton}
        <button
            class="nav-button dashboard-button"
            class:active={dashboardActive}
            onclick={onDashboard}
        >
            <HouseIcon size={16} class="nav-icon" />
            <span>Dashboard</span>
        </button>
    {/if}

    <h2 class="section-title">Tables</h2>
    <div class="filter-input">
        <MagnifyingGlassIcon size={14} class="filter-icon" />
        <input type="text" placeholder="Filter tables..." bind:value={filterText} />
    </div>

    <div class="table-list">
        {#each groupedDisplay.groups as { group, tables: groupTables }}
            <div class="table-group">
                <button class="group-header" onclick={() => toggleGroup(group.name)}>
                    {#if collapsedGroups[group.name]}
                        <CaretRightIcon size={14} class="caret-icon" />
                    {:else}
                        <CaretDownIcon size={14} class="caret-icon" />
                    {/if}
                    {#if group.icon}
                        <DynamicIcon name={group.icon} size={14} class="group-icon" />
                    {/if}
                    <span class="group-name">{group.name}</span>
                    <span class="group-count">{groupTables.length}</span>
                </button>
                {#if !collapsedGroups[group.name]}
                    <ul class="group-tables">
                        {#each groupTables as table}
                            <li>
                                <button
                                    class="nav-button"
                                    class:active={selected === table.name && !dashboardActive}
                                    onclick={() => onSelect(table.name)}
                                >
                                    <DynamicIcon
                                        name={table.icon ?? "table"}
                                        size={16}
                                        class="nav-icon"
                                    />
                                    <span class="truncate"
                                        >{getTableLabel(config, table.name) ?? table.name}</span
                                    >
                                </button>
                            </li>
                        {/each}
                    </ul>
                {/if}
            </div>
        {/each}

        {#if groupedDisplay.ungrouped.length > 0}
            {#if groupedDisplay.groups.length > 0}
                <div class="table-group">
                    <button class="group-header" onclick={() => toggleGroup("__ungrouped__")}>
                        {#if collapsedGroups["__ungrouped__"]}
                            <CaretRightIcon size={14} class="caret-icon" />
                        {:else}
                            <CaretDownIcon size={14} class="caret-icon" />
                        {/if}
                        <span class="group-name">Other</span>
                        <span class="group-count">{groupedDisplay.ungrouped.length}</span>
                    </button>
                    {#if !collapsedGroups["__ungrouped__"]}
                        <ul class="group-tables">
                            {#each groupedDisplay.ungrouped as table}
                                <li>
                                    <button
                                        class="nav-button"
                                        class:active={selected === table.name && !dashboardActive}
                                        onclick={() => onSelect(table.name)}
                                    >
                                        <DynamicIcon
                                            name={table.icon ?? "table"}
                                            size={16}
                                            class="nav-icon"
                                        />
                                        <span class="truncate"
                                            >{getTableLabel(config, table.name) ?? table.name}</span
                                        >
                                    </button>
                                </li>
                            {/each}
                        </ul>
                    {/if}
                </div>
            {:else}
                <!-- No groups configured, show flat list -->
                <ul class="flat-list">
                    {#each groupedDisplay.ungrouped as table}
                        <li>
                            <button
                                class="nav-button"
                                class:active={selected === table.name && !dashboardActive}
                                onclick={() => onSelect(table.name)}
                            >
                                <DynamicIcon
                                    name={table.icon ?? "table"}
                                    size={16}
                                    class="nav-icon"
                                />
                                <span class="truncate"
                                    >{getTableLabel(config, table.name) ?? table.name}</span
                                >
                            </button>
                        </li>
                    {/each}
                </ul>
            {/if}
        {/if}
    </div>

    <div class="settings">
        <button class="settings-button" onclick={toggleTimeMode}>
            <ClockIcon size={14} />
            <span>Times: {timeMode}</span>
        </button>
    </div>
</aside>

<style>
    .sidebar {
        background-color: var(--sidebar);
        padding: 1.5rem;
        overflow-y: auto;
        border-right: 1px solid var(--sidebar-border);
        display: flex;
        flex-direction: column;
        height: 100vh;
        position: sticky;
        top: 0;
    }

    .nav-button {
        width: 100%;
        text-align: left;
        padding: 0.5rem 0.75rem;
        font-size: 0.875rem;
        border-radius: var(--radius-md);
        transition: all 0.15s;
        display: flex;
        align-items: center;
        gap: 0.5rem;
        background: none;
        border: none;
        cursor: pointer;
        color: color-mix(in oklch, var(--sidebar-foreground) 70%, transparent);
    }

    .nav-button:hover {
        color: var(--sidebar-foreground);
        background-color: var(--sidebar-accent);
    }

    .nav-button.active {
        color: var(--sidebar-primary-foreground);
        background-color: var(--sidebar-primary);
        font-weight: 500;
    }

    .nav-button :global(.nav-icon) {
        flex-shrink: 0;
        opacity: 0.7;
    }

    .dashboard-button {
        margin-bottom: 1rem;
    }

    .section-title {
        font-size: 0.75rem;
        font-weight: 500;
        color: color-mix(in oklch, var(--sidebar-foreground) 60%, transparent);
        text-transform: uppercase;
        letter-spacing: 0.1em;
        margin-bottom: 0.75rem;
    }

    .filter-input {
        position: relative;
        margin-bottom: 0.75rem;
    }

    .filter-input :global(.filter-icon) {
        position: absolute;
        left: 0.75rem;
        top: 50%;
        transform: translateY(-50%);
        color: color-mix(in oklch, var(--sidebar-foreground) 50%, transparent);
        pointer-events: none;
    }

    .filter-input input {
        width: 100%;
        padding: 0.5rem 0.75rem 0.5rem 2rem;
        font-size: 0.8125rem;
        border: 1px solid var(--sidebar-border);
        border-radius: var(--radius-md);
        background: color-mix(in oklch, var(--sidebar) 80%, black);
        color: var(--sidebar-foreground);
        outline: none;
        transition: border-color 0.15s;
    }

    .filter-input input::placeholder {
        color: color-mix(in oklch, var(--sidebar-foreground) 40%, transparent);
    }

    .filter-input input:focus {
        border-color: var(--sidebar-primary);
    }

    .table-list {
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
        flex: 1;
    }

    .table-group {
        display: flex;
        flex-direction: column;
    }

    .group-header {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.375rem 0.5rem;
        font-size: 0.75rem;
        font-weight: 500;
        color: color-mix(in oklch, var(--sidebar-foreground) 70%, transparent);
        background: none;
        border: none;
        cursor: pointer;
        border-radius: var(--radius-md);
        transition: all 0.15s;
        text-transform: uppercase;
        letter-spacing: 0.05em;
    }

    .group-header:hover {
        color: var(--sidebar-foreground);
        background-color: var(--sidebar-accent);
    }

    .group-header :global(.caret-icon) {
        flex-shrink: 0;
        opacity: 0.6;
    }

    .group-header :global(.group-icon) {
        flex-shrink: 0;
        opacity: 0.7;
    }

    .group-name {
        flex: 1;
    }

    .group-count {
        font-size: 0.625rem;
        padding: 0.125rem 0.375rem;
        border-radius: 9999px;
        background: color-mix(in oklch, var(--sidebar-foreground) 10%, transparent);
        color: color-mix(in oklch, var(--sidebar-foreground) 60%, transparent);
    }

    .group-tables,
    .flat-list {
        list-style: none;
        padding: 0;
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: 0.125rem;
        padding-left: 0.5rem;
    }

    .flat-list {
        padding-left: 0;
        gap: 0.25rem;
    }

    .truncate {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .settings {
        padding-top: 1rem;
        margin-top: 1rem;
        border-top: 1px solid var(--sidebar-border);
    }

    .settings-button {
        width: 100%;
        text-align: left;
        padding: 0.5rem 0.75rem;
        font-size: 0.75rem;
        border-radius: var(--radius-md);
        transition: all 0.15s;
        display: flex;
        align-items: center;
        gap: 0.5rem;
        background: none;
        border: none;
        cursor: pointer;
        color: color-mix(in oklch, var(--sidebar-foreground) 60%, transparent);
    }

    .settings-button:hover {
        color: var(--sidebar-foreground);
        background-color: var(--sidebar-accent);
    }
</style>
