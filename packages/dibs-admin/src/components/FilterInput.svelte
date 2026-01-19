<script lang="ts">
    import type { ColumnInfo, Filter, FilterOp, Value } from "../types.js";
    import { Command, Popover, Badge, Button } from "../lib/components/ui/index.js";
    import { X } from "phosphor-svelte";

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
        const opMatch = currentToken.match(/^([a-zA-Z_][a-zA-Z0-9_]*)([:=~!<>]=?|[?!])?(.*)$/);

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
        "<": "Lt",
        "<=": "Lte",
        ">": "Gt",
        ">=": "Gte",
        "?": "IsNull",
        "!": "IsNotNull",
    };

    const operatorHelp = [
        { op: ":", desc: "equals", example: "name:john" },
        { op: "~", desc: "contains", example: "name~john" },
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
                    { type: "value" as const, value: "true", label: `${ctx.column}${ctx.op}true`, hint: "boolean" },
                    { type: "value" as const, value: "false", label: `${ctx.column}${ctx.op}false`, hint: "boolean" },
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
        const match = token.match(/^([a-zA-Z_][a-zA-Z0-9_]*)([:=~!<>]=?|[?!])(.*)$/);
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

        return {
            field: colName,
            op: filterOp,
            value: stringToValue(value, col.sql_type),
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
        const opSymbol = Object.entries(opToFilterOp).find(([, v]) => v === filter.op.tag)?.[0] ?? ":";
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

<div class="space-y-3">
    {#if filters.length > 0}
        <div class="flex flex-wrap items-center gap-2">
            {#each filters as filter, i}
                <Badge variant="secondary" class="gap-1.5 py-1 font-mono text-xs">
                    {formatFilter(filter)}
                    <button
                        class="text-muted-foreground hover:text-foreground transition-colors"
                        onclick={() => removeFilter(i)}
                        aria-label="Remove filter"
                    >
                        <X size={12} />
                    </button>
                </Badge>
            {/each}
            <Button variant="ghost" size="sm" class="h-6 px-2 text-xs" onclick={clearFilters}>
                Clear all
            </Button>
        </div>
    {/if}

    <div class="relative">
        <input
            bind:this={inputElement}
            bind:value={inputValue}
            oninput={handleInput}
            onfocus={handleFocus}
            onblur={handleBlur}
            onkeydown={handleKeydown}
            type="text"
            placeholder="Filter: name~john age>18 status:active"
            class="w-full h-9 px-3 py-2 text-sm bg-background border border-input rounded-md
                   placeholder:text-muted-foreground focus:outline-none focus:ring-2
                   focus:ring-ring focus:border-transparent font-mono"
        />

        {#if showSuggestions && suggestions.length > 0}
            <div
                class="absolute top-full left-0 right-0 mt-1 z-50 bg-popover border border-border
                       rounded-md shadow-lg overflow-hidden"
            >
                <div class="max-h-[200px] overflow-y-auto p-1">
                    {#each suggestions as suggestion, i}
                        <button
                            class="w-full flex items-center justify-between px-3 py-2 text-sm rounded
                                   hover:bg-accent hover:text-accent-foreground cursor-pointer
                                   {i === selectedIndex ? 'bg-accent text-accent-foreground' : ''}"
                            onmousedown={() => applySuggestion(suggestion)}
                        >
                            <span class="font-mono">{suggestion.label}</span>
                            <span class="text-xs text-muted-foreground">{suggestion.hint}</span>
                        </button>
                    {/each}
                </div>
                <div class="border-t border-border px-3 py-2 text-xs text-muted-foreground">
                    <kbd class="px-1 bg-muted rounded">Tab</kbd> to complete,
                    <kbd class="px-1 bg-muted rounded">Enter</kbd> to apply
                </div>
            </div>
        {/if}
    </div>
</div>
