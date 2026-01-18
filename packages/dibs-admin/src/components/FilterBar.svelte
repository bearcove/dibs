<script lang="ts">
    import { Plus, X } from "phosphor-svelte";
    import type { ColumnInfo, Filter, FilterOp, Value } from "../types.js";
    import { Button } from "../lib/components/ui/index.js";
    import { Input } from "../lib/components/ui/index.js";
    import { Select } from "../lib/components/ui/index.js";

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
        // Note: "In" needs special handling with multiple values, not yet implemented in UI
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
    <div class="flex flex-wrap items-center gap-3 mb-6">
        {#each filters as filter, i}
            <span
                class="inline-flex items-center gap-2 bg-neutral-900 text-neutral-300 px-3 py-1.5 text-sm"
            >
                {formatFilterDisplay(filter)}
                <button
                    class="text-neutral-500 hover:text-white transition-colors"
                    onclick={() => onRemoveFilter(i)}
                    aria-label="Remove filter"
                >
                    <X size={14} />
                </button>
            </span>
        {/each}
        <Button variant="ghost" size="sm" onclick={onClearFilters}>Clear</Button>
    </div>
{/if}

<div class="flex flex-wrap items-center gap-3 mb-6">
    <Select bind:value={selectedField} class="w-auto">
        {#each columns as col}
            <option value={col.name}>{col.name}</option>
        {/each}
    </Select>

    <Select bind:value={selectedOp} class="w-auto">
        {#each Object.entries(opLabels) as [op, label]}
            <option value={op}>{label}</option>
        {/each}
    </Select>

    {#if needsValue.has(selectedOp)}
        <Input
            type="text"
            bind:value={filterValue}
            placeholder="Value..."
            onkeydown={handleKeydown}
            class="flex-1 min-w-[150px]"
        />
    {/if}

    <Button variant="secondary" size="sm" onclick={addFilter}>
        <Plus size={16} />
        Add Filter
    </Button>
</div>
