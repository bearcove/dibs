<script lang="ts">
    import ArrowSquareOutIcon from "phosphor-svelte/lib/ArrowSquareOutIcon";
    import type { Row, TableInfo, Value, SquelClient } from "@bearcove/dibs-admin/types";
    import {
        getTableByName,
        formatValueForDisplay,
        getDisplayColumn,
    } from "@bearcove/dibs-admin/lib/fk-utils";
    import FkPreview from "./FkPreview.svelte";

    interface Props {
        value: Value;
        fkTable: TableInfo;
        fkColumn: string;
        client: SquelClient;

        onClick: () => void;
        cachedRow?: Row;
    }

    let { value, fkTable, fkColumn, client, onClick, cachedRow }: Props = $props();

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
                table: fkTable.name,
                pk: value,
            });

            if (result.ok) {
                previewRow = result.value;
            } else {
                previewError =
                    result.error.tag === "MigrationFailed"
                        ? result.error.value.message
                        : result.error.value;
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

    // Compute display value: use cached row or preview row's display column if available, otherwise just the PK
    let displayValue = $derived.by(() => {
        const pkStr = formatValueForDisplay(value);

        // Use cachedRow (from batch lookup) or previewRow (from hover fetch) as fallback
        const rowData = cachedRow ?? previewRow;
        if (rowData) {
            const displayCol = getDisplayColumn(fkTable);
            if (displayCol) {
                const displayField = rowData.fields.find((f) => f.name === displayCol.name);
                if (displayField && displayField.value.tag !== "Null") {
                    return formatValueForDisplay(displayField.value);
                }
            }
        }

        return pkStr;
    });
</script>

<button
    class="fk-link"
    onclick={handleClick}
    onmouseenter={handleMouseEnter}
    onmouseleave={handleMouseLeave}
>
    {displayValue}
    <ArrowSquareOutIcon size={12} class="link-icon" />
</button>

{#if showPreview}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
        class="preview-container"
        style="left: {previewPosition.x}px; top: {previewPosition.y}px;"
        onmouseenter={() => (showPreview = true)}
        onmouseleave={handleMouseLeave}
    >
        <FkPreview row={previewRow} table={fkTable} loading={previewLoading} error={previewError} />
    </div>
{/if}

<style>
    .fk-link {
        display: inline-flex;
        align-items: center;
        gap: 0.25rem;
        color: var(--primary);
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        cursor: pointer;
        transition: color 0.15s;
    }

    .fk-link:hover {
        color: color-mix(in oklch, var(--primary) 80%, transparent);
        text-decoration: underline;
    }

    .fk-link :global(.link-icon) {
        opacity: 0.5;
    }

    .preview-container {
        position: fixed;
        z-index: 50;
    }
</style>
