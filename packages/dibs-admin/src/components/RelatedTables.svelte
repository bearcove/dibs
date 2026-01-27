<script lang="ts">
    import type { TableInfo, SchemaInfo, Row, Filter, Value, SquelClient } from "../types.js";
    import { CaretDown, CaretRight, Table as TableIcon } from "phosphor-svelte";
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
        /** Database URL */

        /** Callback when user wants to navigate to a related row */
        onNavigate?: (table: string, pkValue: Value) => void;
    }

    let {
        currentTable,
        currentRow,
        schema,
        client,

        onNavigate,
    }: Props = $props();

    // Track which relations are expanded
    let expandedRelations = $state<Set<string>>(new Set());

    // Track loaded data for each relation
    let relationData = $state<Map<string, { rows: Row[]; total: bigint | null; loading: boolean }>>(
        new Map(),
    );

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
                    // This table has a FK pointing to us
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
        // Priority: explicit label > name > title > first text column
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

    // Toggle a relation's expanded state
    async function toggleRelation(relation: IncomingRelation) {
        const key = `${relation.table.name}:${relation.fkColumns.join(",")}`;

        if (expandedRelations.has(key)) {
            expandedRelations.delete(key);
            expandedRelations = new Set(expandedRelations);
        } else {
            expandedRelations.add(key);
            expandedRelations = new Set(expandedRelations);

            // Load data if not already loaded
            if (!relationData.has(key)) {
                await loadRelationData(relation, key);
            }
        }
    }

    // Get "other" FK columns - FKs that don't point back to the current table
    function getOtherFkColumns(
        relation: IncomingRelation,
    ): { col: string; refTable: string; refCol: string }[] {
        const results: { col: string; refTable: string; refCol: string }[] = [];
        for (const fk of relation.table.foreign_keys) {
            // Skip the FK that points to the current table
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

            // Collect unique PK values to fetch
            const pkValues = new Set<string>();
            for (const row of rows) {
                const field = row.fields.find((f) => f.name === col);
                if (field && field.value.tag !== "Null") {
                    pkValues.add(formatValue(field.value));
                }
            }

            if (pkValues.size === 0) continue;

            // Initialize cache for this table
            if (!newLookup.has(refTable)) {
                newLookup.set(refTable, new Map());
            }
            const tableCache = newLookup.get(refTable)!;

            // Filter out already-cached values
            const uncachedPks = [...pkValues].filter((pk) => !tableCache.has(pk));
            if (uncachedPks.length === 0) continue;

            // Convert to Value array for IN filter
            const inValues = uncachedPks.map((pk) => parsePkValue(pk, pkCol.sql_type));

            // Find display column
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

    // Load data for a relation
    async function loadRelationData(relation: IncomingRelation, key: string) {
        const pkValue = getPkValue();
        if (!pkValue) return;

        relationData.set(key, { rows: [], total: null, loading: true });
        relationData = new Map(relationData);

        try {
            // Build filter: fkColumn = pkValue
            const filters: Filter[] = relation.fkColumns.map((col, i) => ({
                field: col,
                op: { tag: "Eq" as const },
                value: pkValue, // Assuming single-column PK for now
                values: [],
            }));

            const result = await client.list({
                table: relation.table.name,
                filters,
                sort: [],
                limit: 10,
                offset: 0,
                select: [],
            });

            if (result.ok) {
                relationData.set(key, {
                    rows: result.value.rows,
                    total: result.value.total,
                    loading: false,
                });
                relationData = new Map(relationData);

                // Load FK display values for related rows
                await loadFkDisplayValues(result.value.rows, relation);
            } else {
                relationData.set(key, { rows: [], total: null, loading: false });
                relationData = new Map(relationData);
            }
        } catch (e) {
            console.error("Failed to load relation data:", e);
            relationData.set(key, { rows: [], total: null, loading: false });
            relationData = new Map(relationData);
        }
    }

    // Format a value for display
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

    // Get field value from row
    function getFieldValue(row: Row, fieldName: string): Value | null {
        return row.fields.find((f) => f.name === fieldName)?.value ?? null;
    }

    // Get PK value from a related row
    function getRowPk(row: Row, table: TableInfo): Value | null {
        const pkCol = table.columns.find((c) => c.primary_key);
        if (!pkCol) return null;
        return getFieldValue(row, pkCol.name);
    }

    // Check if a column should be excluded from display summary
    function isBoringColumn(
        col: { name: string; sql_type: string; primary_key?: boolean },
        relation: IncomingRelation,
    ): boolean {
        // Skip PKs
        if (col.primary_key) return true;
        // Skip FK columns pointing back to current table
        if (relation.fkColumns.includes(col.name)) return true;
        // Skip timestamp columns
        const nameLower = col.name.toLowerCase();
        if (
            nameLower.includes("created_at") ||
            nameLower.includes("updated_at") ||
            nameLower.includes("deleted_at") ||
            nameLower.includes("_at")
        )
            return true;
        // Skip typical metadata columns
        if (nameLower === "metadata" || nameLower === "raw_data") return true;
        return false;
    }

    // Get the best display string for a related row
    function getRowDisplayString(row: Row, relation: IncomingRelation): string {
        const pkCol = relation.table.columns.find((c) => c.primary_key);

        // First check if there's an explicit label column
        const labelCol = relation.table.columns.find((c) => c.label);
        if (labelCol) {
            const labelValue = getFieldValue(row, labelCol.name);
            if (labelValue && labelValue.tag !== "Null") {
                return formatValue(labelValue);
            }
        }

        // For junction tables, try to show the "other" FK's resolved value
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

        // For non-junction tables, show interesting columns combined
        // E.g., for variant_price: "EUR 19.99" instead of just "EUR"
        const interestingCols = relation.table.columns.filter(
            (col) => !isBoringColumn(col, relation),
        );
        const valueParts: string[] = [];

        for (const col of interestingCols) {
            const value = getFieldValue(row, col.name);
            if (value && value.tag !== "Null") {
                const formatted = formatValue(value);
                // Skip empty strings
                if (formatted && formatted !== "null") {
                    valueParts.push(formatted);
                }
            }
            // Limit to a few columns
            if (valueParts.length >= 3) break;
        }

        if (valueParts.length > 0) {
            return valueParts.join(" · ");
        }

        // Last resort: show the display column
        const displayCol = getDisplayColumn(relation.table);
        const displayValue = getFieldValue(row, displayCol);
        return displayValue ? formatValue(displayValue) : "(no display value)";
    }

    // Get the best navigation target for a row
    // For junction tables, navigate to the "other" side instead of the junction record
    function getNavigationTarget(
        row: Row,
        relation: IncomingRelation,
    ): { table: string; pk: Value } | null {
        const otherFks = getOtherFkColumns(relation);

        // If there's exactly one "other" FK, navigate to that target
        if (otherFks.length === 1) {
            const { col, refTable } = otherFks[0];
            const fkValue = getFieldValue(row, col);
            if (fkValue && fkValue.tag !== "Null") {
                return { table: refTable, pk: fkValue };
            }
        }

        // Otherwise, navigate to the junction table row itself
        const pk = getRowPk(row, relation.table);
        if (pk) {
            return { table: relation.table.name, pk };
        }

        return null;
    }
</script>

{#if incomingRelations.length > 0}
    <div class="mt-6 space-y-2">
        <h3 class="text-sm font-medium text-muted-foreground uppercase tracking-wide">
            Related Records
        </h3>

        {#each incomingRelations as relation}
            {@const key = `${relation.table.name}:${relation.fkColumns.join(",")}`}
            {@const isExpanded = expandedRelations.has(key)}
            {@const data = relationData.get(key)}

            <div class="bg-card text-card-foreground rounded-lg border overflow-hidden">
                <button
                    class="w-full flex items-center gap-2 px-3 py-2 hover:bg-accent/50 transition-colors text-left"
                    onclick={() => toggleRelation(relation)}
                >
                    <span class="text-muted-foreground">
                        {#if isExpanded}
                            <CaretDown size={14} />
                        {:else}
                            <CaretRight size={14} />
                        {/if}
                    </span>

                    {#if relation.table.icon}
                        <DynamicIcon
                            name={relation.table.icon}
                            class="w-4 h-4 text-muted-foreground"
                        />
                    {:else}
                        <TableIcon size={14} class="text-muted-foreground" />
                    {/if}

                    <span class="font-medium text-sm">{relation.table.name}</span>
                    <span class="text-xs text-muted-foreground">
                        via {relation.fkColumns.join(", ")}
                    </span>

                    {#if data && !data.loading && data.total !== null}
                        <span class="ml-auto text-xs text-muted-foreground">
                            {data.total.toString()}
                        </span>
                    {/if}
                </button>

                {#if isExpanded}
                    <div class="border-t border-border">
                        {#if data?.loading}
                            <div class="px-3 py-2 text-sm text-muted-foreground">Loading...</div>
                        {:else if data && data.rows.length > 0}
                            <div class="divide-y divide-border/50">
                                {#each data.rows as row}
                                    {@const target = getNavigationTarget(row, relation)}
                                    <button
                                        class="w-full flex items-center gap-2 px-3 py-1.5 hover:bg-accent/30 transition-colors text-left text-sm"
                                        onclick={() =>
                                            target && onNavigate?.(target.table, target.pk)}
                                    >
                                        <span
                                            class="text-muted-foreground/70 font-mono text-xs min-w-[2rem]"
                                        >
                                            #{target ? formatValue(target.pk) : "?"}
                                        </span>
                                        <span class="truncate">
                                            {getRowDisplayString(row, relation)}
                                        </span>
                                    </button>
                                {/each}

                                {#if data.total !== null && data.total > 10n}
                                    <div class="px-3 py-1.5 text-xs text-muted-foreground">
                                        +{(data.total - 10n).toString()} more
                                    </div>
                                {/if}
                            </div>
                        {:else if data}
                            <div class="px-3 py-2 text-sm text-muted-foreground">
                                No related records
                            </div>
                        {/if}
                    </div>
                {/if}
            </div>
        {/each}
    </div>
{/if}
