<script lang="ts">
    import { Plus, X } from "phosphor-svelte";
    import type { ColumnInfo, Filter, FilterOp, Value } from "../types";
    import { Button, Input, Badge, Select } from "../lib/ui/index";

    interface Props {
        columns: ColumnInfo[];
        filters: Filter[];
        onAddFilter: (filter: Filter) => void;
        onRemoveFilter: (index: number) => void;
        onClearFilters: () => void;
    }

    let { columns, filters, onAddFilter, onRemoveFilter, onClearFilters }: Props = $props();

    let selectedField = $state("");
    let selectedOp = $state<FilterOp["tag"]>("Eq");
    let filterValue = $state("");

    // Update selected field when columns change
    $effect(() => {
        if (
            columns.length > 0 &&
            (!selectedField || !columns.find((c) => c.name === selectedField))
        ) {
            selectedField = columns[0].name;
        }
    });

    const opLabels: Record<FilterOp["tag"], string> = {
        Eq: "=",
        Ne: "≠",
        Lt: "<",
        Lte: "≤",
        Gt: ">",
        Gte: "≥",
        Like: "LIKE",
        ILike: "ILIKE",
        IsNull: "IS NULL",
        IsNotNull: "IS NOT NULL",
        In: "IN",
        JsonGet: "->",
        JsonGetText: "->>",
        Contains: "@>",
        KeyExists: "?",
    };

    const needsValue: Set<FilterOp["tag"]> = new Set([
        "Eq",
        "Ne",
        "Lt",
        "Lte",
        "Gt",
        "Gte",
        "Like",
        "ILike",
    ]);

    function getColumnType(colName: string): string {
        const col = columns.find((c) => c.name === colName);
        return col?.sql_type ?? "text";
    }

    function stringToValue(str: string, sqlType: string): Value {
        const typeLower = sqlType.toLowerCase();

        if (str === "" || str.toLowerCase() === "null") {
            return { tag: "Null" };
        }

        if (typeLower.includes("bool")) {
            return { tag: "Bool", value: str.toLowerCase() === "true" || str === "1" };
        }

        if (typeLower.includes("int8") || typeLower === "bigint" || typeLower === "bigserial") {
            return { tag: "I64", value: BigInt(str) };
        }

        if (typeLower.includes("int4") || typeLower === "integer" || typeLower === "serial") {
            return { tag: "I32", value: parseInt(str, 10) };
        }

        if (typeLower.includes("int2") || typeLower === "smallint" || typeLower === "smallserial") {
            return { tag: "I16", value: parseInt(str, 10) };
        }

        if (typeLower.includes("float4") || typeLower === "real") {
            return { tag: "F32", value: parseFloat(str) };
        }

        if (
            typeLower.includes("float8") ||
            typeLower === "double precision" ||
            typeLower.includes("numeric") ||
            typeLower.includes("decimal")
        ) {
            return { tag: "F64", value: parseFloat(str) };
        }

        return { tag: "String", value: str };
    }

    function addFilter() {
        const op: FilterOp = { tag: selectedOp } as FilterOp;
        const sqlType = getColumnType(selectedField);
        const value = needsValue.has(selectedOp)
            ? stringToValue(filterValue, sqlType)
            : { tag: "Null" as const };

        onAddFilter({
            field: selectedField,
            op,
            value,
            values: [],
        });

        filterValue = "";
    }

    function formatFilterDisplay(filter: Filter): string {
        const opLabel = opLabels[filter.op.tag];
        if (!needsValue.has(filter.op.tag)) {
            return `${filter.field} ${opLabel}`;
        }
        const valueStr =
            filter.value.tag === "Null"
                ? "null"
                : typeof filter.value.value === "bigint"
                  ? filter.value.value.toString()
                  : JSON.stringify(filter.value.value);
        return `${filter.field} ${opLabel} ${valueStr}`;
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Enter") {
            addFilter();
        }
    }
</script>

{#if filters.length > 0}
    <div class="active-filters">
        {#each filters as filter, i}
            <Badge variant="secondary" class="filter-badge">
                {formatFilterDisplay(filter)}
                <button
                    class="remove-filter"
                    onclick={() => onRemoveFilter(i)}
                    aria-label="Remove filter"
                >
                    <X size={14} />
                </button>
            </Badge>
        {/each}
        <Button variant="ghost" size="sm" onclick={onClearFilters}>Clear</Button>
    </div>
{/if}

<div class="filter-row">
    <Select.Root type="single" bind:value={selectedField}>
        <Select.Trigger class="select-field">
            {selectedField || "Select column..."}
        </Select.Trigger>
        <Select.Content>
            {#each columns as col}
                <Select.Item value={col.name}>{col.name}</Select.Item>
            {/each}
        </Select.Content>
    </Select.Root>

    <Select.Root type="single" bind:value={selectedOp}>
        <Select.Trigger class="select-op">
            {opLabels[selectedOp]}
        </Select.Trigger>
        <Select.Content>
            {#each Object.entries(opLabels) as [op, label]}
                <Select.Item value={op}>{label}</Select.Item>
            {/each}
        </Select.Content>
    </Select.Root>

    {#if needsValue.has(selectedOp)}
        <Input
            type="text"
            bind:value={filterValue}
            placeholder="Value..."
            onkeydown={handleKeydown}
            class="value-input"
        />
    {/if}

    <Button variant="secondary" size="sm" onclick={addFilter}>
        <Plus size={16} />
        Add Filter
    </Button>
</div>

<style>
    .active-filters {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 0.75rem;
        margin-bottom: 1.5rem;
    }

    :global(.filter-badge) {
        gap: 0.5rem;
        padding-block: 0.375rem;
    }

    .remove-filter {
        background: none;
        border: none;
        padding: 0;
        cursor: pointer;
        color: var(--muted-foreground);
        transition: color 0.15s;
    }

    .remove-filter:hover {
        color: var(--foreground);
    }

    .filter-row {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 0.75rem;
        margin-bottom: 1.5rem;
    }

    :global(.select-field) {
        width: 180px;
    }

    :global(.select-op) {
        width: 120px;
    }

    :global(.value-input) {
        flex: 1;
        min-width: 150px;
    }
</style>
