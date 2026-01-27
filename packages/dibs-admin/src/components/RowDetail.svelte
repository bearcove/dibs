<script lang="ts">
    import { Trash, ArrowLeft, FloppyDisk, ArrowCounterClockwise, Asterisk } from "phosphor-svelte";
    import type {
        Row,
        RowField,
        ColumnInfo,
        Value,
        TableInfo,
        SchemaInfo,
        SquelClient,
    } from "../types";
    import type { DibsAdminConfig, DetailConfig, FieldGroupConfig } from "../types/config";
    import { Button, Label, Tooltip } from "../lib/ui/index";
    import { Info } from "phosphor-svelte";
    import {
        getDetailConfig,
        isFieldReadOnly,
        isFieldHidden,
        shouldShowRelations,
        getTableLabel,
    } from "../lib/config";
    import { getFkForColumn, getTableByName } from "../lib/fk-utils";
    import InlineField from "./InlineField.svelte";
    import FkSelect from "./FkSelect.svelte";
    import DynamicIcon from "./DynamicIcon.svelte";
    import RelatedTables from "./RelatedTables.svelte";
    import ConfirmChangesDialog from "./ConfirmChangesDialog.svelte";

    interface Props {
        columns: ColumnInfo[];
        row: Row;
        table: TableInfo;
        schema: SchemaInfo;
        client: SquelClient;

        tableName: string;
        config?: DibsAdminConfig;
        onFieldSave?: (fieldName: string, newValue: Value) => void | Promise<void>;
        onDelete?: () => void;
        onClose: () => void;
        deleting?: boolean;
        onNavigate?: (table: string, pkValue: Value) => void;
    }

    let {
        columns,
        row,
        table,
        schema,
        client,

        tableName,
        config,
        onFieldSave,
        onDelete,
        onClose,
        deleting = false,
        onNavigate,
    }: Props = $props();

    // Get detail config for this table
    let detailConfig = $derived(getDetailConfig(config, tableName));

    // Get the display/label value for this record
    let recordDisplayValue = $derived.by(() => {
        // First, look for a column marked with dibs::label
        const labelCol = columns.find((c) => c.label);
        if (labelCol) {
            const field = row.fields.find((f) => f.name === labelCol.name);
            if (field && field.value.tag !== "Null") {
                return valueToString(field.value);
            }
        }
        // Fallback: look for common display column names
        const displayNames = ["name", "title", "display_name", "username", "label", "email"];
        for (const name of displayNames) {
            const col = columns.find((c) => c.name === name);
            if (col) {
                const field = row.fields.find((f) => f.name === name);
                if (field && field.value.tag !== "Null") {
                    return valueToString(field.value);
                }
            }
        }
        return null;
    });

    // Track pending changes (field name -> new string value)
    let pendingChanges = $state<Map<string, string>>(new Map());

    // Confirmation dialog state
    let showConfirmDialog = $state(false);
    let saving = $state(false);
    let saveError = $state<string | null>(null);

    // Check if there are unsaved changes
    let hasChanges = $derived(pendingChanges.size > 0);

    // Build the list of changes for the confirmation dialog
    let changesList = $derived.by(() => {
        const list: { field: string; label: string; oldValue: string; newValue: string }[] = [];
        for (const [fieldName, newValue] of pendingChanges) {
            const col = columns.find((c) => c.name === fieldName);
            const oldValue = getOriginalFieldValue(fieldName);
            list.push({
                field: fieldName,
                label: col?.doc || fieldName,
                oldValue,
                newValue,
            });
        }
        return list;
    });

    // Determine visible columns based on config
    let visibleColumns = $derived(() => {
        if (detailConfig?.fields) {
            // If fields are specified, use them in order
            const result: (ColumnInfo | FieldGroupConfig)[] = [];
            for (const field of detailConfig.fields) {
                if (typeof field === "string") {
                    const col = columns.find((c) => c.name === field);
                    if (col && !isFieldHidden(field, detailConfig)) {
                        result.push(col);
                    }
                } else {
                    // It's a FieldGroupConfig
                    result.push(field);
                }
            }
            return result;
        }
        // Otherwise show all non-hidden columns
        return columns.filter((c) => !isFieldHidden(c.name, detailConfig));
    });

    function valueToString(value: Value): string {
        if (value.tag === "Null") return "";
        if (typeof value.value === "bigint") return value.value.toString();
        if (value.tag === "Bytes") return "<bytes>";
        if (value.tag === "Bool") return value.value ? "true" : "false";
        return String(value.value);
    }

    function stringToValue(str: string, sqlType: string): Value {
        const typeLower = sqlType.toLowerCase();

        if (str === "") {
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

    // Get the ORIGINAL field value from the row (not pending changes)
    function getOriginalFieldValue(colName: string): string {
        const field = row.fields.find((f) => f.name === colName);
        return field ? valueToString(field.value) : "";
    }

    // Get the CURRENT field value (considering pending changes)
    function getFieldValue(colName: string): string {
        if (pendingChanges.has(colName)) {
            return pendingChanges.get(colName)!;
        }
        return getOriginalFieldValue(colName);
    }

    function getControlType(
        col: ColumnInfo,
    ): "checkbox" | "number" | "datetime" | "textarea" | "text" | "enum" | "codemirror" {
        const typeLower = col.sql_type.toLowerCase();

        if (col.lang) {
            return "codemirror";
        }

        if (col.enum_variants.length > 0) {
            return "enum";
        }

        if (typeLower.includes("bool")) {
            return "checkbox";
        }

        if (typeLower.includes("timestamp") || typeLower.includes("datetime")) {
            return "datetime";
        }

        if (
            typeLower.includes("int") ||
            typeLower.includes("serial") ||
            typeLower.includes("float") ||
            typeLower.includes("numeric") ||
            typeLower.includes("decimal") ||
            typeLower.includes("real") ||
            typeLower === "double precision"
        ) {
            return "number";
        }

        if (col.long || typeLower.includes("json")) {
            return "textarea";
        }

        return "text";
    }

    function mapControlTypeToInlineType(
        controlType: ReturnType<typeof getControlType>,
    ): "text" | "number" | "boolean" | "datetime" | "enum" | "textarea" | "codemirror" {
        switch (controlType) {
            case "checkbox":
                return "boolean";
            case "number":
                return "number";
            case "datetime":
                return "datetime";
            case "textarea":
                return "textarea";
            case "enum":
                return "enum";
            case "codemirror":
                return "codemirror";
            default:
                return "text";
        }
    }

    function isColumnReadOnly(col: ColumnInfo): boolean {
        // Primary key and auto-generated columns are always read-only for existing rows
        if (col.primary_key || col.auto_generated) return true;
        // Check config
        return isFieldReadOnly(col.name, detailConfig);
    }

    // Check if a column is required (non-nullable, no default, not auto-generated)
    function isColumnRequired(col: ColumnInfo): boolean {
        if (col.auto_generated) return false;
        return !col.nullable && !col.default;
    }

    // Track a pending change (don't save yet)
    function handleFieldChange(colName: string, newStrValue: string) {
        const originalValue = getOriginalFieldValue(colName);
        if (newStrValue === originalValue) {
            // Value reverted to original - remove from pending changes
            pendingChanges.delete(colName);
            pendingChanges = new Map(pendingChanges); // trigger reactivity
        } else {
            pendingChanges.set(colName, newStrValue);
            pendingChanges = new Map(pendingChanges); // trigger reactivity
        }
        saveError = null;
    }

    // Discard all pending changes
    function discardChanges() {
        pendingChanges = new Map();
        saveError = null;
    }

    // Show confirmation dialog
    function handleSaveClick() {
        showConfirmDialog = true;
    }

    // Actually save all pending changes
    async function confirmSave() {
        saving = true;
        saveError = null;

        try {
            // Save each changed field
            for (const [fieldName, newStrValue] of pendingChanges) {
                const col = columns.find((c) => c.name === fieldName);
                if (col) {
                    const newValue = stringToValue(newStrValue, col.sql_type);
                    await onFieldSave?.(fieldName, newValue);
                }
            }
            // Clear pending changes on success
            pendingChanges = new Map();
            showConfirmDialog = false;
        } catch (e) {
            saveError = e instanceof Error ? e.message : String(e);
        } finally {
            saving = false;
        }
    }

    function handleDelete() {
        if (onDelete && confirm("Are you sure you want to delete this row?")) {
            onDelete();
        }
    }

    // FK support
    function getFkInfo(col: ColumnInfo): { fkTable: TableInfo } | null {
        const fk = getFkForColumn(table, col.name);
        if (!fk) return null;
        const targetTable = getTableByName(schema, fk.references_table);
        if (!targetTable) return null;
        return { fkTable: targetTable };
    }

    function getLangIcon(lang: string | null | undefined): string | null {
        if (!lang) return null;
        switch (lang.toLowerCase()) {
            case "markdown":
            case "md":
                return "file-text";
            case "json":
                return "braces";
            case "html":
                return "file-code";
            default:
                return "code";
        }
    }

    // Check if column is a ColumnInfo (not FieldGroupConfig)
    function isColumnInfo(item: ColumnInfo | FieldGroupConfig): item is ColumnInfo {
        return "name" in item && "sql_type" in item;
    }
</script>

<div class="detail-panel">
    <!-- Header with back button -->
    <header class="panel-header">
        <div class="header-left">
            <Button variant="ghost" size="icon" onclick={onClose}>
                <ArrowLeft size={20} />
            </Button>
            <div>
                <h1 class="panel-title">
                    <DynamicIcon name={table.icon ?? "table"} size={20} class="title-icon" />
                    {recordDisplayValue ?? tableName}
                </h1>
            </div>
        </div>

        <!-- Actions -->
        <div class="header-actions">
            {#if saveError}
                <span class="error-text">{saveError}</span>
            {/if}

            {#if hasChanges}
                <Button variant="ghost" size="sm" onclick={discardChanges}>
                    <ArrowCounterClockwise size={16} />
                    Discard
                </Button>
                <Button variant="default" size="sm" onclick={handleSaveClick}>
                    <FloppyDisk size={16} />
                    Save Changes ({pendingChanges.size})
                </Button>
            {/if}

            {#if onDelete}
                <Button variant="destructive" size="sm" onclick={handleDelete} disabled={deleting}>
                    <Trash size={16} />
                    {deleting ? "Deleting..." : "Delete"}
                </Button>
            {/if}
        </div>
    </header>

    <!-- Scrollable content -->
    <div class="panel-content">
        <div class="fields-container">
            {#each visibleColumns() as item}
                {#if isColumnInfo(item)}
                    {@const col = item}
                    {@const controlType = getControlType(col)}
                    {@const inlineType = mapControlTypeToInlineType(controlType)}
                    {@const readOnly = isColumnReadOnly(col)}
                    {@const required = isColumnRequired(col)}
                    {@const fkInfo = getFkInfo(col)}
                    {@const langIcon = getLangIcon(col.lang)}
                    {@const fieldValue = getFieldValue(col.name)}
                    {@const isModified = pendingChanges.has(col.name)}
                    {@const tooltipContent = [
                        col.sql_type,
                        col.primary_key ? "primary key" : null,
                        col.doc,
                    ]
                        .filter(Boolean)
                        .join(" · ")}

                    {#if inlineType === "boolean"}
                        <!-- Boolean fields: checkbox with label inline on same line -->
                        <label class="field field-boolean" class:modified={isModified}>
                            <InlineField
                                value={fieldValue}
                                type="boolean"
                                {readOnly}
                                onchange={(v) => handleFieldChange(col.name, v)}
                            />
                            <span class="boolean-text" class:modified-label={isModified}
                                >{col.doc || col.name}</span
                            >
                            {#if isModified}
                                <span class="modified-indicator">modified</span>
                            {/if}
                        </label>
                    {:else}
                        <!-- Other fields: label above, value below -->
                        <div class="field" class:modified={isModified}>
                            <div class="field-label">
                                {#if langIcon}
                                    <DynamicIcon name={langIcon} size={14} class="field-icon" />
                                {:else if col.icon}
                                    <DynamicIcon name={col.icon} size={14} class="field-icon" />
                                {/if}
                                <Label class={isModified ? "modified-label" : ""}
                                    >{col.doc || col.name}</Label
                                >
                                {#if required}
                                    <Asterisk size={10} class="required-indicator" weight="bold" />
                                {/if}
                                {#if isModified}
                                    <span class="modified-indicator">modified</span>
                                {/if}
                                <Tooltip.Root>
                                    <Tooltip.Trigger>
                                        {#snippet children({ props })}
                                            {@const { tabindex: _, ...restProps } = props as Record<
                                                string,
                                                unknown
                                            >}
                                            <span {...restProps} class="info-trigger" tabindex={-1}>
                                                <Info size={12} />
                                            </span>
                                        {/snippet}
                                    </Tooltip.Trigger>
                                    <Tooltip.Content>
                                        <p class="tooltip-text">
                                            <span class="mono">{col.name}</span> · {tooltipContent}
                                        </p>
                                    </Tooltip.Content>
                                </Tooltip.Root>
                            </div>
                            <div class="field-value">
                                {#if fkInfo && !readOnly}
                                    <FkSelect
                                        value={fieldValue}
                                        fkTable={fkInfo.fkTable}
                                        {client}
                                        disabled={readOnly}
                                        onchange={(v) => handleFieldChange(col.name, v)}
                                    />
                                {:else}
                                    <InlineField
                                        value={fieldValue}
                                        type={inlineType}
                                        {readOnly}
                                        placeholder={col.nullable ? "null" : ""}
                                        enumOptions={col.enum_variants}
                                        lang={col.lang}
                                        onchange={(v) => handleFieldChange(col.name, v)}
                                    />
                                {/if}
                            </div>
                        </div>
                    {/if}
                {:else}
                    <!-- Field group -->
                    {@const group = item}
                    <details class="field-group" open={!group.collapsed}>
                        <summary class="group-title">
                            {group.title}
                        </summary>
                        <div class="group-fields">
                            {#each group.fields as fieldName}
                                {@const col = columns.find((c) => c.name === fieldName)}
                                {#if col}
                                    {@const controlType = getControlType(col)}
                                    {@const inlineType = mapControlTypeToInlineType(controlType)}
                                    {@const readOnly = isColumnReadOnly(col)}
                                    {@const fkInfo = getFkInfo(col)}
                                    {@const fieldValue = getFieldValue(col.name)}
                                    {@const isModified = pendingChanges.has(col.name)}

                                    {#if inlineType === "boolean"}
                                        <label
                                            class="field field-boolean"
                                            class:modified={isModified}
                                        >
                                            <InlineField
                                                value={fieldValue}
                                                type="boolean"
                                                {readOnly}
                                                onchange={(v) => handleFieldChange(col.name, v)}
                                            />
                                            <span
                                                class="boolean-text"
                                                class:modified-label={isModified}
                                                >{col.doc || col.name}</span
                                            >
                                            {#if isModified}
                                                <span class="modified-indicator">modified</span>
                                            {/if}
                                        </label>
                                    {:else}
                                        <div class="field" class:modified={isModified}>
                                            <div class="field-label">
                                                <Label class={isModified ? "modified-label" : ""}>
                                                    {col.doc || col.name}
                                                    {#if isModified}
                                                        <span class="modified-indicator"
                                                            >modified</span
                                                        >
                                                    {/if}
                                                </Label>
                                            </div>
                                            <div class="field-value">
                                                {#if fkInfo && !readOnly}
                                                    <FkSelect
                                                        value={fieldValue}
                                                        fkTable={fkInfo.fkTable}
                                                        {client}
                                                        disabled={readOnly}
                                                        onchange={(v) =>
                                                            handleFieldChange(col.name, v)}
                                                    />
                                                {:else}
                                                    <InlineField
                                                        value={fieldValue}
                                                        type={inlineType}
                                                        {readOnly}
                                                        placeholder={col.nullable ? "null" : ""}
                                                        enumOptions={col.enum_variants}
                                                        lang={col.lang}
                                                        onchange={(v) =>
                                                            handleFieldChange(col.name, v)}
                                                    />
                                                {/if}
                                            </div>
                                        </div>
                                    {/if}
                                {/if}
                            {/each}
                        </div>
                    </details>
                {/if}
            {/each}
        </div>

        <!-- Related tables section -->
        {#if shouldShowRelations(detailConfig)}
            <div class="related-section">
                <RelatedTables
                    currentTable={table}
                    currentRow={row}
                    {schema}
                    {client}
                    {onNavigate}
                />
            </div>
        {/if}
    </div>
</div>

<!-- Confirmation dialog -->
<ConfirmChangesDialog
    bind:open={showConfirmDialog}
    changes={changesList}
    {saving}
    onconfirm={confirmSave}
    oncancel={() => (showConfirmDialog = false)}
/>

<style>
    .detail-panel {
        height: 100%;
        max-height: 100vh;
        display: flex;
        flex-direction: column;
        background-color: var(--background);
        overflow: hidden;
    }

    .panel-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
        padding: 1rem 1.5rem;
        border-bottom: 1px solid var(--border);
        flex-shrink: 0;
        max-width: 44rem;
    }

    @media (min-width: 768px) {
        .panel-header {
            padding: 1rem 2rem;
        }
    }

    .header-left {
        display: flex;
        align-items: center;
        gap: 1rem;
    }

    .panel-title {
        font-size: 1.125rem;
        font-weight: 500;
        color: var(--foreground);
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin: 0;
    }

    :global(.title-icon) {
        opacity: 0.7;
    }

    .header-actions {
        display: flex;
        align-items: center;
        gap: 0.75rem;
    }

    .error-text {
        font-size: 0.75rem;
        color: var(--destructive);
    }

    .panel-content {
        flex: 1;
        min-height: 0;
        overflow-y: auto;
        padding: 1.5rem;
    }

    @media (min-width: 768px) {
        .panel-content {
            padding: 2rem;
        }
    }

    /* Fields container - simple block layout with fixed width */
    .fields-container {
        width: 36rem;
        max-width: 100%;
    }

    /* Individual field - label on top, value below */
    .field {
        margin-bottom: 1rem;
    }

    .field.modified {
        background-color: oklch(from var(--accent) l c h / 0.1);
        margin-left: -1rem;
        margin-right: -1rem;
        padding: 0.5rem 1rem;
        border-radius: 0.375rem;
    }

    .field-label {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        margin-bottom: 0.5rem;
    }

    .field-value {
        /* Children (InlineField, FkSelect) handle their own styling */
    }

    /* Boolean fields - checkbox and label inline on same line */
    .field-boolean {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        cursor: pointer;
        margin-bottom: 1rem;
    }

    .boolean-text {
        font-size: 0.875rem;
        color: var(--foreground);
        user-select: none;
    }

    :global(.field-icon) {
        color: var(--muted-foreground);
        opacity: 0.6;
    }

    :global(.modified-label) {
        color: var(--primary);
    }

    :global(.required-indicator) {
        color: var(--destructive);
        flex-shrink: 0;
    }

    .modified-indicator {
        font-size: 0.625rem;
        color: var(--primary);
        font-weight: 500;
        margin-left: 0.25rem;
    }

    .info-trigger {
        cursor: help;
        color: var(--muted-foreground);
        opacity: 0.4;
        transition: opacity 0.15s;
    }

    .info-trigger:hover {
        opacity: 1;
    }

    .tooltip-text {
        font-size: 0.75rem;
    }

    .mono {
        font-family: ui-monospace, monospace;
    }

    /* Field groups (collapsible sections) */
    .field-group {
        margin-bottom: 1rem;
    }

    .group-title {
        cursor: pointer;
        font-size: 0.7rem;
        font-weight: 600;
        color: var(--muted-foreground);
        text-transform: uppercase;
        letter-spacing: 0.05em;
        padding: 0.25rem 0;
        margin-bottom: 0.75rem;
    }

    .group-fields {
        /* Fields inside groups use same .field class */
    }

    .related-section {
        margin-top: 2rem;
        width: 36rem;
        max-width: 100%;
    }
</style>
