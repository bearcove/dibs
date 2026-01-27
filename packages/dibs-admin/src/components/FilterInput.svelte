<script lang="ts">
    import type { ColumnInfo, Filter, FilterOp, Value } from "@bearcove/dibs-admin/types";
    import { Badge, Button } from "@bearcove/dibs-admin/lib/ui";
    import XIcon from "phosphor-svelte/lib/XIcon";

    interface Props {
        columns: ColumnInfo[];
        filters: Filter[];
        onFiltersChange: (filters: Filter[]) => void;
    }

    let { columns, filters, onFiltersChange }: Props = $props();

    let inputValue = $state("");
    let inputElement: HTMLInputElement | null = $state(null);
    let showSuggestions = $state(false);
    let selectedIndex = $state(0);

    // Parse the current input to understand context
    type ParseContext =
        | { type: "column"; partial: string }
        | { type: "operator"; column: string; partial: string }
        | { type: "value"; column: string; op: string; partial: string };

    function getParseContext(input: string): ParseContext {
        // Find the last "token" being typed (after last space)
        const lastSpace = input.lastIndexOf(" ");
        const currentToken = lastSpace >= 0 ? input.slice(lastSpace + 1) : input;

        // Check for operator patterns
        // Operators: : = != ~ ~~ < <= > >= ? !
        const opMatch = currentToken.match(/^([a-zA-Z_][a-zA-Z0-9_]*)(~~|[:=~<>]=?|!=|[?!])?(.*)$/);

        if (!opMatch) {
            return { type: "column", partial: currentToken };
        }

        const [, colName, op, value] = opMatch;

        if (!op) {
            return { type: "column", partial: colName };
        }

        if (value === "" && !["?", "!"].includes(op)) {
            return { type: "operator", column: colName, partial: op };
        }

        return { type: "value", column: colName, op, partial: value };
    }

    // Operator mappings
    const opToFilterOp: Record<string, FilterOp["tag"]> = {
        ":": "Eq",
        "=": "Eq",
        "!=": "Ne",
        "~": "ILike",
        "~~": "Like",
        "<": "Lt",
        "<=": "Lte",
        ">": "Gt",
        ">=": "Gte",
        "?": "IsNull",
        "!": "IsNotNull",
    };

    const operatorHelp = [
        { op: ":", desc: "equals", example: "name:john" },
        { op: "~", desc: "contains (case-insensitive)", example: "name~john" },
        { op: "~~", desc: "contains (case-sensitive)", example: "name~~John" },
        { op: ">", desc: "greater than", example: "age>18" },
        { op: ">=", desc: "greater or equal", example: "age>=18" },
        { op: "<", desc: "less than", example: "age<100" },
        { op: "<=", desc: "less or equal", example: "age<=100" },
        { op: "!=", desc: "not equals", example: "status!=deleted" },
        { op: "?", desc: "is null", example: "deleted?" },
        { op: "!", desc: "is not null", example: "email!" },
    ];

    // Get suggestions based on current context
    let suggestions = $derived.by(() => {
        const ctx = getParseContext(inputValue);

        if (ctx.type === "column") {
            const partial = ctx.partial.toLowerCase();
            return columns
                .filter((c) => c.name.toLowerCase().includes(partial))
                .map((c) => ({
                    type: "column" as const,
                    value: c.name,
                    label: c.name,
                    hint: c.sql_type,
                }));
        }

        if (ctx.type === "operator") {
            return operatorHelp.map((o) => ({
                type: "operator" as const,
                value: o.op,
                label: `${ctx.column}${o.op}`,
                hint: o.desc,
            }));
        }

        if (ctx.type === "value") {
            const col = columns.find((c) => c.name === ctx.column);
            if (col?.enum_variants && col.enum_variants.length > 0) {
                const partial = ctx.partial.toLowerCase();
                return col.enum_variants
                    .filter((v) => v.toLowerCase().includes(partial))
                    .map((v) => ({
                        type: "value" as const,
                        value: v,
                        label: `${ctx.column}${ctx.op}${v}`,
                        hint: "enum value",
                    }));
            }
            // For boolean columns
            if (col?.sql_type.toLowerCase().includes("bool")) {
                return [
                    {
                        type: "value" as const,
                        value: "true",
                        label: `${ctx.column}${ctx.op}true`,
                        hint: "boolean",
                    },
                    {
                        type: "value" as const,
                        value: "false",
                        label: `${ctx.column}${ctx.op}false`,
                        hint: "boolean",
                    },
                ];
            }
        }

        return [];
    });

    // Apply a suggestion
    function applySuggestion(suggestion: (typeof suggestions)[0]) {
        const lastSpace = inputValue.lastIndexOf(" ");
        const prefix = lastSpace >= 0 ? inputValue.slice(0, lastSpace + 1) : "";

        if (suggestion.type === "column") {
            inputValue = prefix + suggestion.value;
            // Keep suggestions open for operator
            showSuggestions = true;
        } else {
            inputValue = prefix + suggestion.label;
            showSuggestions = false;
        }

        selectedIndex = 0;
        inputElement?.focus();
    }

    // Parse a single filter token
    function parseFilterToken(token: string): Filter | null {
        const match = token.match(/^([a-zA-Z_][a-zA-Z0-9_]*)(~~|[:=~<>]=?|!=|[?!])(.*)$/);
        if (!match) return null;

        const [, colName, op, value] = match;
        const col = columns.find((c) => c.name === colName);
        if (!col) return null;

        const filterOpTag = opToFilterOp[op];
        if (!filterOpTag) return null;

        const filterOp: FilterOp = { tag: filterOpTag } as FilterOp;

        // Null checks don't need values
        if (filterOpTag === "IsNull" || filterOpTag === "IsNotNull") {
            return {
                field: colName,
                op: filterOp,
                value: { tag: "Null" },
                values: [],
            };
        }

        // For LIKE/ILIKE, wrap value with % for "contains" behavior
        // unless user already included wildcards
        let finalValue = value;
        if ((filterOpTag === "Like" || filterOpTag === "ILike") && !value.includes("%")) {
            finalValue = `%${value}%`;
        }

        return {
            field: colName,
            op: filterOp,
            value: stringToValue(finalValue, col.sql_type),
            values: [],
        };
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

    // Handle Enter to apply filters
    function handleKeydown(e: KeyboardEvent) {
        if (showSuggestions && suggestions.length > 0) {
            if (e.key === "ArrowDown") {
                e.preventDefault();
                selectedIndex = Math.min(selectedIndex + 1, suggestions.length - 1);
                return;
            }
            if (e.key === "ArrowUp") {
                e.preventDefault();
                selectedIndex = Math.max(selectedIndex - 1, 0);
                return;
            }
            if (e.key === "Tab" || (e.key === "Enter" && suggestions.length > 0)) {
                e.preventDefault();
                applySuggestion(suggestions[selectedIndex]);
                return;
            }
            if (e.key === "Escape") {
                showSuggestions = false;
                return;
            }
        }

        if (e.key === "Enter") {
            applyFilters();
        }
    }

    function applyFilters() {
        const tokens = inputValue.trim().split(/\s+/).filter(Boolean);
        const newFilters: Filter[] = [];

        for (const token of tokens) {
            const filter = parseFilterToken(token);
            if (filter) {
                newFilters.push(filter);
            }
        }

        if (newFilters.length > 0) {
            onFiltersChange([...filters, ...newFilters]);
            inputValue = "";
            showSuggestions = false;
        }
    }

    function removeFilter(index: number) {
        const newFilters = [...filters];
        newFilters.splice(index, 1);
        onFiltersChange(newFilters);
    }

    function clearFilters() {
        onFiltersChange([]);
    }

    function formatFilter(filter: Filter): string {
        const opSymbol =
            Object.entries(opToFilterOp).find(([, v]) => v === filter.op.tag)?.[0] ?? ":";
        if (filter.op.tag === "IsNull") return `${filter.field}?`;
        if (filter.op.tag === "IsNotNull") return `${filter.field}!`;
        const valueStr =
            filter.value.tag === "Null"
                ? "null"
                : typeof filter.value.value === "bigint"
                  ? filter.value.value.toString()
                  : String(filter.value.value);
        return `${filter.field}${opSymbol}${valueStr}`;
    }

    function handleInput() {
        showSuggestions = inputValue.length > 0;
        selectedIndex = 0;
    }

    function handleFocus() {
        if (inputValue.length > 0) {
            showSuggestions = true;
        }
    }

    function handleBlur(e: FocusEvent) {
        // Delay hiding to allow clicking on suggestions
        setTimeout(() => {
            showSuggestions = false;
        }, 150);
    }
</script>

<div class="filter-input-container">
    {#if filters.length > 0}
        <div class="active-filters">
            {#each filters as filter, i}
                <Badge variant="secondary" class="filter-badge">
                    {formatFilter(filter)}
                    <button
                        class="remove-btn"
                        onclick={() => removeFilter(i)}
                        aria-label="Remove filter"
                    >
                        <XIcon size={12} />
                    </button>
                </Badge>
            {/each}
            <Button variant="ghost" size="sm" class="clear-btn" onclick={clearFilters}>
                Clear all
            </Button>
        </div>
    {/if}

    <div class="input-wrapper">
        <input
            bind:this={inputElement}
            bind:value={inputValue}
            oninput={handleInput}
            onfocus={handleFocus}
            onblur={handleBlur}
            onkeydown={handleKeydown}
            type="text"
            placeholder="Filter: name~john age>18 status:active"
            class="filter-input"
        />

        {#if showSuggestions && suggestions.length > 0}
            <div class="suggestions-dropdown">
                <div class="suggestions-list">
                    {#each suggestions as suggestion, i}
                        <button
                            class="suggestion-item"
                            class:selected={i === selectedIndex}
                            onmousedown={() => applySuggestion(suggestion)}
                        >
                            <span class="suggestion-label">{suggestion.label}</span>
                            <span class="suggestion-hint">{suggestion.hint}</span>
                        </button>
                    {/each}
                </div>
                <div class="suggestions-help">
                    <kbd>Tab</kbd> to complete,
                    <kbd>Enter</kbd> to apply
                </div>
            </div>
        {/if}
    </div>
</div>

<style>
    .filter-input-container {
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
    }

    .active-filters {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 0.5rem;
    }

    :global(.filter-badge) {
        gap: 0.375rem;
        padding-block: 0.25rem;
        font-family: ui-monospace, monospace;
        font-size: 0.75rem;
    }

    .remove-btn {
        background: none;
        border: none;
        padding: 0;
        cursor: pointer;
        color: var(--muted-foreground);
        transition: color 0.15s;
    }

    .remove-btn:hover {
        color: var(--foreground);
    }

    :global(.clear-btn) {
        height: 1.5rem;
        padding: 0 0.5rem;
        font-size: 0.75rem;
    }

    .input-wrapper {
        position: relative;
    }

    .filter-input {
        width: 100%;
        height: 2.25rem;
        padding: 0.5rem 0.75rem;
        font-size: 0.875rem;
        font-family: ui-monospace, monospace;
        background-color: var(--background);
        border: 1px solid var(--input);
        border-radius: var(--radius-md, 0.375rem);
        color: var(--foreground);
        transition:
            border-color 0.15s,
            outline 0.15s;
    }

    .filter-input::placeholder {
        color: var(--muted-foreground);
    }

    .filter-input:focus {
        outline: 2px solid var(--ring);
        outline-offset: 0;
        border-color: transparent;
    }

    .suggestions-dropdown {
        position: absolute;
        top: 100%;
        left: 0;
        right: 0;
        margin-top: 0.25rem;
        z-index: 50;
        background-color: var(--popover);
        border: 1px solid var(--border);
        border-radius: var(--radius-md, 0.375rem);
        box-shadow:
            0 4px 6px -1px rgb(0 0 0 / 0.1),
            0 2px 4px -2px rgb(0 0 0 / 0.1);
        overflow: hidden;
    }

    .suggestions-list {
        max-height: 200px;
        overflow-y: auto;
        padding: 0.25rem;
    }

    .suggestion-item {
        width: 100%;
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 0.5rem 0.75rem;
        font-size: 0.875rem;
        border-radius: var(--radius-sm, 0.25rem);
        border: none;
        background: transparent;
        cursor: pointer;
        text-align: left;
        color: var(--foreground);
    }

    .suggestion-item:hover,
    .suggestion-item.selected {
        background-color: var(--accent);
        color: var(--accent-foreground);
    }

    .suggestion-label {
        font-family: ui-monospace, monospace;
    }

    .suggestion-hint {
        font-size: 0.75rem;
        color: var(--muted-foreground);
    }

    .suggestion-item:hover .suggestion-hint,
    .suggestion-item.selected .suggestion-hint {
        color: inherit;
        opacity: 0.7;
    }

    .suggestions-help {
        border-top: 1px solid var(--border);
        padding: 0.5rem 0.75rem;
        font-size: 0.75rem;
        color: var(--muted-foreground);
    }

    .suggestions-help kbd {
        padding: 0.125rem 0.25rem;
        background-color: var(--muted);
        border-radius: var(--radius-sm, 0.25rem);
    }
</style>
