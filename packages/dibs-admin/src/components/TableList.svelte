<script lang="ts">
    import { Clock, House } from "phosphor-svelte";
    import type { TableInfo } from "../types";
    import type { DibsAdminConfig } from "../types/config";
    import { getTableLabel } from "../lib/config";
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
            <House size={16} class="nav-icon" />
            <span>Dashboard</span>
        </button>
    {/if}

    <h2 class="section-title">Tables</h2>
    <ul class="table-list">
        {#each tables as table}
            <li>
                <button
                    class="nav-button"
                    class:active={selected === table.name && !dashboardActive}
                    onclick={() => onSelect(table.name)}
                >
                    <DynamicIcon name={table.icon ?? "table"} size={16} class="nav-icon" />
                    <span class="truncate">{getTableLabel(config, table.name) ?? table.name}</span>
                </button>
            </li>
        {/each}
    </ul>

    <div class="settings">
        <button class="settings-button" onclick={toggleTimeMode}>
            <Clock size={14} />
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
        min-height: 100vh;
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
        margin-bottom: 1rem;
    }

    .table-list {
        list-style: none;
        padding: 0;
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        flex: 1;
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
