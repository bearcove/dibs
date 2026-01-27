<script lang="ts">
    import type {
        TableInfo,
        SchemaInfo,
        Row,
        Filter,
        Value,
        SquelClient,
    } from "@bearcove/dibs-admin/types";
    import CaretDownIcon from "phosphor-svelte/lib/CaretDownIcon";
    import CaretRightIcon from "phosphor-svelte/lib/CaretRightIcon";
    import DynamicIcon from "./DynamicIcon.svelte";

    interface IncomingRelation {
        /** The table that has a FK pointing to us */
        table: TableInfo;
        /** The FK column(s) in that table */
        fkColumns: string[];
        /** What column(s) they reference in our table */
        referencedColumns: string[];
        /** Human-readable label for this relation */
        label: string;
    }

    interface Props {
        /** Current table we're viewing */
        currentTable: TableInfo;
        /** The row we're viewing (to get PK value) */
        currentRow: Row;
        /** Full schema for looking up relations */
        schema: SchemaInfo;
        /** Client for fetching data */
        client: SquelClient;
        /** Callback when user wants to navigate to a related row */
        onNavigate?: (table: string, pkValue: Value) => void;
    }

    let { currentTable, currentRow, schema, client, onNavigate }: Props = $props();

    // Track which relations are collapsed (default is expanded)
    let collapsedRelations = $state<Set<string>>(new Set());

    // Track loaded data for each relation
    let relationData = $state<
        Map<string, { rows: Row[]; total: bigint | null; loading: boolean; loadingMore: boolean }>
    >(new Map());

    // FK lookup cache for resolving foreign key display values
    let fkLookup = $state<Map<string, Map<string, Row>>>(new Map());

    // Compute incoming relations for the current table
    let incomingRelations = $derived.by(() => {
        const relations: IncomingRelation[] = [];
        const currentTableName = currentTable.name;

        for (const table of schema.tables) {
            if (table.name === currentTableName) continue;

            for (const fk of table.foreign_keys) {
                if (fk.references_table === currentTableName) {
                    const label = `${table.name} (${fk.columns.join(", ")})`;
                    relations.push({
                        table,
                        fkColumns: fk.columns,
                        referencedColumns: fk.references_columns,
                        label,
                    });
                }
            }
        }

        return relations;
    });

    // Get PK value from current row
    function getPkValue(): Value | null {
        const pkCol = currentTable.columns.find((c) => c.primary_key);
        if (!pkCol) return null;
        const field = currentRow.fields.find((f) => f.name === pkCol.name);
        return field?.value ?? null;
    }

    // Get display column for a table
    function getDisplayColumn(table: TableInfo): string {
        const labelCol = table.columns.find((c) => c.label);
        if (labelCol) return labelCol.name;

        const nameCol = table.columns.find((c) => c.name === "name");
        if (nameCol) return nameCol.name;

        const titleCol = table.columns.find((c) => c.name === "title");
        if (titleCol) return titleCol.name;

        const textCol = table.columns.find(
            (c) =>
                c.sql_type.toLowerCase().includes("text") ||
                c.sql_type.toLowerCase().includes("varchar"),
        );
        if (textCol) return textCol.name;

        return table.columns[0]?.name ?? "id";
    }

    // Preload all relations on mount
    $effect(() => {
        // Re-run when currentRow changes (viewing a different record)
        const _rowId = getPkValue();

        for (const relation of incomingRelations) {
            const key = `${relation.table.name}:${relation.fkColumns.join(",")}`;
            if (!relationData.has(key)) {
                loadRelationData(relation, key);
            }
        }
    });

    // Toggle a relation's collapsed state
    function toggleRelation(relation: IncomingRelation) {
        const key = `${relation.table.name}:${relation.fkColumns.join(",")}`;

        if (collapsedRelations.has(key)) {
            collapsedRelations.delete(key);
            collapsedRelations = new Set(collapsedRelations);
        } else {
            collapsedRelations.add(key);
            collapsedRelations = new Set(collapsedRelations);
        }
    }

    // Get "other" FK columns - FKs that don't point back to the current table
    function getOtherFkColumns(
        relation: IncomingRelation,
    ): { col: string; refTable: string; refCol: string }[] {
        const results: { col: string; refTable: string; refCol: string }[] = [];
        for (const fk of relation.table.foreign_keys) {
            if (fk.references_table === currentTable.name) continue;
            for (let i = 0; i < fk.columns.length; i++) {
                results.push({
                    col: fk.columns[i],
                    refTable: fk.references_table,
                    refCol: fk.references_columns[i],
                });
            }
        }
        return results;
    }

    // Load FK display values for related rows
    async function loadFkDisplayValues(rows: Row[], relation: IncomingRelation) {
        const otherFks = getOtherFkColumns(relation);
        if (otherFks.length === 0) return;

        const newLookup = new Map(fkLookup);

        for (const { col, refTable } of otherFks) {
            const refTableInfo = schema.tables.find((t) => t.name === refTable);
            if (!refTableInfo) continue;

            const pkCol = refTableInfo.columns.find((c) => c.primary_key);
            if (!pkCol) continue;

            const pkValues = new Set<string>();
            for (const row of rows) {
                const field = row.fields.find((f) => f.name === col);
                if (field && field.value.tag !== "Null") {
                    pkValues.add(formatValue(field.value));
                }
            }

            if (pkValues.size === 0) continue;

            if (!newLookup.has(refTable)) {
                newLookup.set(refTable, new Map());
            }
            const tableCache = newLookup.get(refTable)!;

            const uncachedPks = [...pkValues].filter((pk) => !tableCache.has(pk));
            if (uncachedPks.length === 0) continue;

            const inValues = uncachedPks.map((pk) => parsePkValue(pk, pkCol.sql_type));

            const labelCol = refTableInfo.columns.find((c) => c.label);
            const displayCol =
                labelCol ??
                refTableInfo.columns.find((c) =>
                    [
                        "name",
                        "title",
                        "label",
                        "display_name",
                        "username",
                        "email",
                        "slug",
                    ].includes(c.name.toLowerCase()),
                );

            const selectCols = [pkCol.name];
            if (displayCol && displayCol.name !== pkCol.name) {
                selectCols.push(displayCol.name);
            }

            try {
                const result = await client.list({
                    table: refTable,
                    filters: [
                        {
                            field: pkCol.name,
                            op: { tag: "In" },
                            value: { tag: "Null" },
                            values: inValues,
                        },
                    ],
                    sort: [],
                    limit: inValues.length,
                    offset: null,
                    select: selectCols,
                });

                if (result.ok) {
                    for (const row of result.value.rows) {
                        const pkField = row.fields.find((f) => f.name === pkCol.name);
                        if (pkField) {
                            tableCache.set(formatValue(pkField.value), row);
                        }
                    }
                }
            } catch (e) {
                console.error(`Failed to load FK values for ${refTable}:`, e);
            }
        }

        fkLookup = newLookup;
    }

    function parsePkValue(str: string, sqlType: string): Value {
        const typeLower = sqlType.toLowerCase();
        if (typeLower.includes("int8") || typeLower === "bigint" || typeLower === "bigserial") {
            return { tag: "I64", value: BigInt(str) };
        }
        if (typeLower.includes("int")) {
            return { tag: "I32", value: parseInt(str, 10) };
        }
        return { tag: "String", value: str };
    }

    const PAGE_SIZE = 10;

    // Load data for a relation
    async function loadRelationData(relation: IncomingRelation, key: string) {
        const pkValue = getPkValue();
        if (!pkValue) return;

        relationData.set(key, { rows: [], total: null, loading: true, loadingMore: false });
        relationData = new Map(relationData);

        try {
            const filters: Filter[] = relation.fkColumns.map((col, i) => ({
                field: col,
                op: { tag: "Eq" as const },
                value: pkValue,
                values: [],
            }));

            const result = await client.list({
                table: relation.table.name,
                filters,
                sort: [],
                limit: PAGE_SIZE,
                offset: 0,
                select: [],
            });

            if (result.ok) {
                relationData.set(key, {
                    rows: result.value.rows,
                    total: result.value.total,
                    loading: false,
                    loadingMore: false,
                });
                relationData = new Map(relationData);

                await loadFkDisplayValues(result.value.rows, relation);
            } else {
                relationData.set(key, {
                    rows: [],
                    total: null,
                    loading: false,
                    loadingMore: false,
                });
                relationData = new Map(relationData);
            }
        } catch (e) {
            console.error("Failed to load relation data:", e);
            relationData.set(key, { rows: [], total: null, loading: false, loadingMore: false });
            relationData = new Map(relationData);
        }
    }

    // Load more rows for a relation
    async function loadMore(relation: IncomingRelation, key: string) {
        const pkValue = getPkValue();
        if (!pkValue) return;

        const existing = relationData.get(key);
        if (!existing || existing.loadingMore) return;

        relationData.set(key, { ...existing, loadingMore: true });
        relationData = new Map(relationData);

        try {
            const filters: Filter[] = relation.fkColumns.map((col, i) => ({
                field: col,
                op: { tag: "Eq" as const },
                value: pkValue,
                values: [],
            }));

            const result = await client.list({
                table: relation.table.name,
                filters,
                sort: [],
                limit: PAGE_SIZE,
                offset: existing.rows.length,
                select: [],
            });

            if (result.ok) {
                const newRows = [...existing.rows, ...result.value.rows];
                relationData.set(key, {
                    rows: newRows,
                    total: result.value.total,
                    loading: false,
                    loadingMore: false,
                });
                relationData = new Map(relationData);

                await loadFkDisplayValues(result.value.rows, relation);
            } else {
                relationData.set(key, { ...existing, loadingMore: false });
                relationData = new Map(relationData);
            }
        } catch (e) {
            console.error("Failed to load more relation data:", e);
            relationData.set(key, { ...existing, loadingMore: false });
            relationData = new Map(relationData);
        }
    }

    function formatValue(value: Value): string {
        if (value.tag === "Null") return "null";
        if (value.tag === "Bool") return value.value ? "true" : "false";
        if (value.tag === "I64") return value.value.toString();
        if (value.tag === "String") {
            const s = value.value;
            return s.length > 50 ? s.slice(0, 50) + "..." : s;
        }
        if ("value" in value) return String(value.value);
        return "";
    }

    function getFieldValue(row: Row, fieldName: string): Value | null {
        return row.fields.find((f) => f.name === fieldName)?.value ?? null;
    }

    function getRowPk(row: Row, table: TableInfo): Value | null {
        const pkCol = table.columns.find((c) => c.primary_key);
        if (!pkCol) return null;
        return getFieldValue(row, pkCol.name);
    }

    function isBoringColumn(
        col: { name: string; sql_type: string; primary_key?: boolean },
        relation: IncomingRelation,
    ): boolean {
        if (col.primary_key) return true;
        if (relation.fkColumns.includes(col.name)) return true;
        const nameLower = col.name.toLowerCase();
        if (
            nameLower.includes("created_at") ||
            nameLower.includes("updated_at") ||
            nameLower.includes("deleted_at") ||
            nameLower.includes("_at")
        )
            return true;
        if (nameLower === "metadata" || nameLower === "raw_data") return true;
        return false;
    }

    function getRowDisplayString(row: Row, relation: IncomingRelation): string {
        const pkCol = relation.table.columns.find((c) => c.primary_key);

        const labelCol = relation.table.columns.find((c) => c.label);
        if (labelCol) {
            const labelValue = getFieldValue(row, labelCol.name);
            if (labelValue && labelValue.tag !== "Null") {
                return formatValue(labelValue);
            }
        }

        const otherFks = getOtherFkColumns(relation);
        if (otherFks.length > 0) {
            const displayParts: string[] = [];

            for (const { col, refTable } of otherFks) {
                const fkValue = getFieldValue(row, col);
                if (!fkValue || fkValue.tag === "Null") continue;

                const fkStr = formatValue(fkValue);
                const tableCache = fkLookup.get(refTable);
                const cachedRow = tableCache?.get(fkStr);

                if (cachedRow) {
                    const refTableInfo = schema.tables.find((t) => t.name === refTable);
                    if (refTableInfo) {
                        const refDisplayCol = getDisplayColumn(refTableInfo);
                        const refDisplayValue = getFieldValue(cachedRow, refDisplayCol);
                        if (refDisplayValue) {
                            displayParts.push(formatValue(refDisplayValue));
                            continue;
                        }
                    }
                }

                displayParts.push(`${refTable}#${fkStr}`);
            }

            if (displayParts.length > 0) {
                return displayParts.join(" → ");
            }
        }

        const interestingCols = relation.table.columns.filter(
            (col) => !isBoringColumn(col, relation),
        );
        const valueParts: string[] = [];

        for (const col of interestingCols) {
            const value = getFieldValue(row, col.name);
            if (value && value.tag !== "Null") {
                const formatted = formatValue(value);
                if (formatted && formatted !== "null") {
                    valueParts.push(formatted);
                }
            }
            if (valueParts.length >= 3) break;
        }

        if (valueParts.length > 0) {
            return valueParts.join(" · ");
        }

        const displayCol = getDisplayColumn(relation.table);
        const displayValue = getFieldValue(row, displayCol);
        return displayValue ? formatValue(displayValue) : "(no display value)";
    }

    function getNavigationTarget(
        row: Row,
        relation: IncomingRelation,
    ): { table: string; pk: Value } | null {
        const otherFks = getOtherFkColumns(relation);

        if (otherFks.length === 1) {
            const { col, refTable } = otherFks[0];
            const fkValue = getFieldValue(row, col);
            if (fkValue && fkValue.tag !== "Null") {
                return { table: refTable, pk: fkValue };
            }
        }

        const pk = getRowPk(row, relation.table);
        if (pk) {
            return { table: relation.table.name, pk };
        }

        return null;
    }
</script>

{#if incomingRelations.length > 0}
    {@const relationsWithData = incomingRelations.filter((r) => {
        const key = `${r.table.name}:${r.fkColumns.join(",")}`;
        const data = relationData.get(key);
        return !data || data.loading || (data.total !== null && data.total > 0n);
    })}
    {#if relationsWithData.length > 0}
        <div class="related-tables">
            <h3 class="section-title">Related Records</h3>

            {#each relationsWithData as relation}
                {@const key = `${relation.table.name}:${relation.fkColumns.join(",")}`}
                {@const isExpanded = !collapsedRelations.has(key)}
                {@const data = relationData.get(key)}
                {@const remainingCount =
                    data && data.total !== null ? data.total - BigInt(data.rows.length) : 0n}

                <div class="relation-card">
                    <button class="relation-header" onclick={() => toggleRelation(relation)}>
                        <span class="caret-icon">
                            {#if isExpanded}
                                <CaretDownIcon size={14} />
                            {:else}
                                <CaretRightIcon size={14} />
                            {/if}
                        </span>

                        <DynamicIcon
                            name={relation.table.icon ?? "table"}
                            size={14}
                            class="table-icon"
                        />

                        <span class="table-name">{relation.table.name}</span>
                        <span class="via-text">via {relation.fkColumns.join(", ")}</span>

                        {#if data && !data.loading && data.total !== null}
                            <span class="count">{data.total.toString()}</span>
                        {/if}
                    </button>

                    {#if isExpanded}
                        <div class="relation-content">
                            {#if data?.loading}
                                <div class="loading-message">Loading...</div>
                            {:else if data && data.rows.length > 0}
                                <div class="relation-rows">
                                    {#each data.rows as row}
                                        {@const target = getNavigationTarget(row, relation)}
                                        <button
                                            class="relation-row"
                                            onclick={() =>
                                                target && onNavigate?.(target.table, target.pk)}
                                        >
                                            <span class="row-id">
                                                #{target ? formatValue(target.pk) : "?"}
                                            </span>
                                            <span class="row-label">
                                                {getRowDisplayString(row, relation)}
                                            </span>
                                        </button>
                                    {/each}

                                    {#if remainingCount > 0n}
                                        <button
                                            class="load-more-button"
                                            onclick={() => loadMore(relation, key)}
                                            disabled={data.loadingMore}
                                        >
                                            {#if data.loadingMore}
                                                Loading...
                                            {:else}
                                                Load more ({remainingCount.toString()} remaining)
                                            {/if}
                                        </button>
                                    {/if}
                                </div>
                            {:else if data}
                                <div class="empty-message">No related records</div>
                            {/if}
                        </div>
                    {/if}
                </div>
            {/each}
        </div>
    {/if}
{/if}

<style>
    .related-tables {
        margin-top: 1.5rem;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .section-title {
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--muted-foreground);
        text-transform: uppercase;
        letter-spacing: 0.05em;
        margin: 0 0 0.5rem 0;
    }

    .relation-card {
        background-color: var(--card);
        color: var(--card-foreground);
        border-radius: var(--radius-lg);
        border: 1px solid var(--border);
        overflow: hidden;
    }

    .relation-header {
        width: 100%;
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.5rem 0.75rem;
        background: none;
        border: none;
        cursor: pointer;
        text-align: left;
        font: inherit;
        color: var(--card-foreground);
        transition: background-color 0.15s;
    }

    .relation-header:hover {
        background-color: color-mix(in oklch, var(--accent) 50%, transparent);
    }

    .caret-icon {
        color: var(--muted-foreground);
    }

    .relation-header :global(.table-icon) {
        width: 1rem;
        height: 1rem;
        color: var(--muted-foreground);
    }

    .table-name {
        font-weight: 500;
        font-size: 0.875rem;
    }

    .via-text {
        font-size: 0.75rem;
        color: var(--muted-foreground);
    }

    .count {
        margin-left: auto;
        font-size: 0.75rem;
        color: var(--muted-foreground);
    }

    .relation-content {
        border-top: 1px solid var(--border);
    }

    .loading-message,
    .empty-message {
        padding: 0.5rem 0.75rem;
        font-size: 0.875rem;
        color: var(--muted-foreground);
    }

    .relation-rows {
        display: flex;
        flex-direction: column;
    }

    .relation-rows > * + * {
        border-top: 1px solid color-mix(in oklch, var(--border) 50%, transparent);
    }

    .relation-row {
        width: 100%;
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding: 0.375rem 0.75rem;
        background: none;
        border: none;
        cursor: pointer;
        text-align: left;
        font: inherit;
        font-size: 0.875rem;
        color: var(--card-foreground);
        transition: background-color 0.15s;
    }

    .relation-row:hover {
        background-color: color-mix(in oklch, var(--accent) 30%, transparent);
    }

    .row-id {
        color: color-mix(in oklch, var(--muted-foreground) 70%, transparent);
        font-family: ui-monospace, monospace;
        font-size: 0.75rem;
        min-width: 2rem;
    }

    .row-label {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .load-more-button {
        width: 100%;
        padding: 0.5rem 0.75rem;
        font-size: 0.75rem;
        color: var(--muted-foreground);
        background: none;
        border: none;
        cursor: pointer;
        text-align: center;
        font: inherit;
        transition:
            background-color 0.15s,
            color 0.15s;
    }

    .load-more-button:hover:not(:disabled) {
        background-color: color-mix(in oklch, var(--accent) 30%, transparent);
        color: var(--foreground);
    }

    .load-more-button:disabled {
        cursor: default;
        opacity: 0.6;
    }
</style>
