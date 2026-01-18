<script lang="ts">
    import { ArrowSquareOut } from "phosphor-svelte";
    import type { Row, TableInfo, Value, SquelClient } from "../types.js";
    import { getTableByName, formatValueForDisplay, getDisplayColumn } from "../lib/fk-utils.js";
    import FkPreview from "./FkPreview.svelte";

    interface Props {
        value: Value;
        fkTable: TableInfo;
        fkColumn: string;
        client: SquelClient;
        databaseUrl: string;
        onClick: () => void;
        cachedRow?: Row;
    }

    let { value, fkTable, fkColumn, client, databaseUrl, onClick, cachedRow }: Props = $props();

    // Hover preview state
    let showPreview = $state(false);
    let previewRow = $state<Row | null>(null);
    let previewLoading = $state(false);
    let previewError = $state<string | null>(null);
    let hoverTimeout: ReturnType<typeof setTimeout> | null = null;
    let previewPosition = $state({ x: 0, y: 0 });

    async function loadPreview() {
        if (value.tag === "Null") return;

        previewLoading = true;
        previewError = null;

        try {
            const result = await client.get({
                database_url: databaseUrl,
                table: fkTable.name,
                pk: value,
            });

            if (result.ok) {
                previewRow = result.value;
            } else {
                previewError = result.error.value;
            }
        } catch (e) {
            previewError = e instanceof Error ? e.message : String(e);
        } finally {
            previewLoading = false;
        }
    }

    function handleMouseEnter(e: MouseEvent) {
        // Debounce hover
        hoverTimeout = setTimeout(() => {
            const rect = (e.target as HTMLElement).getBoundingClientRect();
            previewPosition = { x: rect.left, y: rect.bottom + 4 };
            showPreview = true;

            // Use cached row if available, otherwise fetch
            if (cachedRow) {
                previewRow = cachedRow;
            } else if (!previewRow && !previewLoading) {
                loadPreview();
            }
        }, 300);
    }

    function handleMouseLeave() {
        if (hoverTimeout) {
            clearTimeout(hoverTimeout);
            hoverTimeout = null;
        }
        showPreview = false;
    }

    function handleClick(e: MouseEvent) {
        e.stopPropagation();
        onClick();
    }

    // Compute display value: use cached row's display column if available, otherwise just the PK
    let displayValue = $derived.by(() => {
        const pkStr = formatValueForDisplay(value);

        if (cachedRow) {
            const displayCol = getDisplayColumn(fkTable);
            if (displayCol) {
                const displayField = cachedRow.fields.find(f => f.name === displayCol.name);
                if (displayField && displayField.value.tag !== "Null") {
                    return formatValueForDisplay(displayField.value);
                }
            }
        }

        return pkStr;
    });
</script>

<button
    class="inline-flex items-center gap-1 text-blue-400 hover:text-blue-300 hover:underline transition-colors cursor-pointer"
    onclick={handleClick}
    onmouseenter={handleMouseEnter}
    onmouseleave={handleMouseLeave}
>
    {displayValue}
    <ArrowSquareOut size={12} class="opacity-50" />
</button>

{#if showPreview}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
        class="fixed z-50"
        style="left: {previewPosition.x}px; top: {previewPosition.y}px;"
        onmouseenter={() => showPreview = true}
        onmouseleave={handleMouseLeave}
    >
        <FkPreview
            row={previewRow}
            table={fkTable}
            loading={previewLoading}
            error={previewError}
        />
    </div>
{/if}
