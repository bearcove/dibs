<script lang="ts">
    import { ArrowRight } from "phosphor-svelte";
    import type { LatestRecordsTile } from "../../types/config";
    import type { SchemaInfo, SquelClient, Row, Value } from "../../types";
    import { Card } from "../../lib/ui/index";
    import DynamicIcon from "../DynamicIcon.svelte";

    interface Props {
        config: LatestRecordsTile;
        schema: SchemaInfo;
        client: SquelClient;

        onSelectTable: (tableName: string) => void;
    }

    let { config, schema, client, onSelectTable }: Props = $props();

    let rows = $state<Row[]>([]);
    let loading = $state(true);
    let error = $state<string | null>(null);

    let tableInfo = $derived(schema.tables.find((t) => t.name === config.table));
    let title = $derived(config.title ?? `Recent ${config.table}`);
    let limit = $derived(config.limit ?? 5);

    // Determine which columns to show
    let displayColumns = $derived(() => {
        if (config.columns && config.columns.length > 0) {
            return config.columns;
        }
        // Default: show label column and first few columns
        if (!tableInfo) return [];
        const labelCol = tableInfo.columns.find((c) => c.label);
        const pkCol = tableInfo.columns.find((c) => c.primary_key);
        const cols = [pkCol?.name, labelCol?.name].filter(Boolean) as string[];
        // Add a couple more columns if available
        for (const col of tableInfo.columns) {
            if (cols.length >= 3) break;
            if (!cols.includes(col.name) && !col.long) {
                cols.push(col.name);
            }
        }
        return cols;
    });

    // Determine sort
    let sortConfig = $derived(() => {
        if (config.sort) {
            return {
                field: config.sort.field,
                dir:
                    config.sort.direction === "desc"
                        ? { tag: "Desc" as const }
                        : { tag: "Asc" as const },
            };
        }
        // Default: sort by created_at or PK desc
        if (tableInfo) {
            const createdAt = tableInfo.columns.find(
                (c) => c.name === "created_at" || c.name === "createdat",
            );
            if (createdAt) {
                return { field: createdAt.name, dir: { tag: "Desc" as const } };
            }
            const pk = tableInfo.columns.find((c) => c.primary_key);
            if (pk) {
                return { field: pk.name, dir: { tag: "Desc" as const } };
            }
        }
        return null;
    });

    $effect(() => {
        loadData();
    });

    async function loadData() {
        if (!config.table) return;

        loading = true;
        error = null;

        try {
            const result = await client.list({
                table: config.table,
                filters: [],
                sort: sortConfig() ? [sortConfig()!] : [],
                limit,
                offset: null,
                select: displayColumns(),
            });

            if (result.ok) {
                rows = result.value.rows;
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

    function formatValue(value: Value): string {
        if (value.tag === "Null") return "—";
        if (typeof value.value === "bigint") return value.value.toString();
        if (value.tag === "Bool") return value.value ? "Yes" : "No";
        if (value.tag === "String" && value.value.length > 40) {
            return value.value.slice(0, 40) + "…";
        }
        return String(value.value);
    }

    function getRowLabel(row: Row): string {
        // Try to get a meaningful label
        const labelCol = tableInfo?.columns.find((c) => c.label);
        if (labelCol) {
            const field = row.fields.find((f) => f.name === labelCol.name);
            if (field) return formatValue(field.value);
        }
        // Fall back to first string column or PK
        const firstStr = row.fields.find((f) => f.value.tag === "String");
        if (firstStr) return formatValue(firstStr.value);
        const pkCol = tableInfo?.columns.find((c) => c.primary_key);
        if (pkCol) {
            const field = row.fields.find((f) => f.name === pkCol.name);
            if (field) return `#${formatValue(field.value)}`;
        }
        return "—";
    }

    function getRowSubtitle(row: Row): string | null {
        // Try to get a secondary value (like created_at)
        const createdAt = row.fields.find((f) => f.name === "created_at" || f.name === "createdat");
        if (createdAt && createdAt.value.tag === "String") {
            const date = new Date(createdAt.value.value);
            if (!isNaN(date.getTime())) {
                return formatRelativeTime(date);
            }
        }
        return null;
    }

    function formatRelativeTime(date: Date): string {
        const now = new Date();
        const diffMs = now.getTime() - date.getTime();
        const diffSec = Math.floor(diffMs / 1000);
        const diffMin = Math.floor(diffSec / 60);
        const diffHour = Math.floor(diffMin / 60);
        const diffDay = Math.floor(diffHour / 24);

        if (diffSec < 60) return "just now";
        if (diffMin < 60) return `${diffMin}m ago`;
        if (diffHour < 24) return `${diffHour}h ago`;
        if (diffDay < 30) return `${diffDay}d ago`;
        return date.toLocaleDateString();
    }
</script>

<Card.Root class="tile-card">
    <Card.Header class="tile-header">
        <Card.Title class="tile-title">
            <span class="title-content">
                {#if tableInfo?.icon}
                    <DynamicIcon name={tableInfo.icon} size={16} class="title-icon" />
                {/if}
                {title}
            </span>
            <button class="view-all-btn" onclick={() => onSelectTable(config.table)}>
                <ArrowRight size={16} />
            </button>
        </Card.Title>
    </Card.Header>
    <Card.Content class="tile-content">
        {#if loading}
            <div class="status-text">Loading...</div>
        {:else if error}
            <div class="error-text">{error}</div>
        {:else if rows.length === 0}
            <div class="empty-text">No records</div>
        {:else}
            <ul class="records-list">
                {#each rows as row}
                    {@const label = getRowLabel(row)}
                    {@const subtitle = getRowSubtitle(row)}
                    <li class="record-item">
                        <div class="record-label">{label}</div>
                        {#if subtitle}
                            <div class="record-subtitle">{subtitle}</div>
                        {/if}
                    </li>
                {/each}
            </ul>
        {/if}
    </Card.Content>
</Card.Root>

<style>
    :global(.tile-card) {
        display: flex;
        flex-direction: column;
    }

    :global(.tile-header) {
        padding-bottom: 0.75rem;
    }

    :global(.tile-title) {
        display: flex;
        align-items: center;
        justify-content: space-between;
        font-size: 0.875rem;
        font-weight: 500;
    }

    .title-content {
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    :global(.title-icon) {
        color: var(--muted-foreground);
    }

    .view-all-btn {
        background: none;
        border: none;
        padding: 0;
        cursor: pointer;
        color: var(--muted-foreground);
        transition: color 0.15s;
    }

    .view-all-btn:hover {
        color: var(--foreground);
    }

    :global(.tile-content) {
        flex: 1;
    }

    .status-text {
        font-size: 0.875rem;
        color: var(--muted-foreground);
    }

    .error-text {
        font-size: 0.875rem;
        color: var(--destructive);
    }

    .empty-text {
        font-size: 0.875rem;
        color: oklch(from var(--muted-foreground) l c h / 0.6);
    }

    .records-list {
        list-style: none;
        padding: 0;
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .record-item {
        font-size: 0.875rem;
    }

    .record-label {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .record-subtitle {
        font-size: 0.75rem;
        color: var(--muted-foreground);
    }
</style>
