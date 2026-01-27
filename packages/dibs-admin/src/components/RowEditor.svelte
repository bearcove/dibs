<script lang="ts">
    import {
        Trash,
        ArrowSquareOut,
        Asterisk,
        ArrowLeft,
        Info,
        ArrowCounterClockwise,
    } from "phosphor-svelte";
    import type {
        Row,
        RowField,
        ColumnInfo,
        Value,
        TableInfo,
        SchemaInfo,
        SquelClient,
    } from "../types";
    import {
        Button,
        Input,
        NumberInput,
        DatetimeInput,
        Textarea,
        Checkbox,
        Label,
        Dialog,
        Select,
        Tooltip,
    } from "../lib/ui/index";
    import { getFkForColumn, getTableByName } from "../lib/fk-utils";
    import FkSelect from "./FkSelect.svelte";
    import DynamicIcon from "./DynamicIcon.svelte";
    import CodeMirrorEditor from "./CodeMirrorEditor.svelte";
    import RelatedTables from "./RelatedTables.svelte";

    interface Props {
        columns: ColumnInfo[];
        row: Row | null; // null = creating new row
        onSave: (data: Row, dirtyFields?: Set<string>) => void;
        onDelete?: () => void;
        onClose: () => void;
        saving?: boolean;
        deleting?: boolean;
        // FK support
        table?: TableInfo;
        schema?: SchemaInfo;
        client?: SquelClient;

        // Display mode
        fullscreen?: boolean;
        tableName?: string;
        // Navigation callback for related records
        onNavigate?: (table: string, pkValue: Value) => void;
    }

    let {
        columns,
        row,
        onSave,
        onDelete,
        onClose,
        saving = false,
        deleting = false,
        table,
        schema,
        client,

        fullscreen = false,
        tableName = "",
        onNavigate,
    }: Props = $props();

    // Form state - map column name to string value
    let formValues = $state<Map<string, string>>(new Map());

    // Original values for dirty comparison
    let originalValues = $state<Map<string, string>>(new Map());

    // Track validation errors for required fields
    let validationErrors = $state<Map<string, string>>(new Map());

    // Initialize form values from row or empty
    $effect(() => {
        const newValues = new Map<string, string>();
        for (const col of columns) {
            if (row) {
                const field = row.fields.find((f) => f.name === col.name);
                if (field) {
                    newValues.set(col.name, valueToString(field.value));
                } else {
                    newValues.set(col.name, "");
                }
            } else {
                newValues.set(col.name, "");
            }
        }
        formValues = newValues;
        originalValues = new Map(newValues);
        validationErrors = new Map();
    });

    // Compute dirty fields by comparing current values to original
    let dirtyFields = $derived.by(() => {
        const dirty = new Set<string>();
        for (const [key, value] of formValues) {
            const original = originalValues.get(key) ?? "";
            if (value !== original) {
                dirty.add(key);
            }
        }
        return dirty;
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

    function handleSave() {
        // For inserts: validate required fields
        if (!row) {
            const errors = new Map<string, string>();
            for (const col of columns) {
                if (isRequired(col)) {
                    const value = formValues.get(col.name) ?? "";
                    if (value === "") {
                        errors.set(col.name, "Required");
                    }
                }
            }
            if (errors.size > 0) {
                validationErrors = errors;
                return;
            }
        }

        const fields: RowField[] = [];
        for (const col of columns) {
            // Skip auto-generated fields when creating
            if (!row && col.auto_generated) continue;

            // For updates: only include dirty fields
            if (row && !dirtyFields.has(col.name)) continue;

            const strValue = formValues.get(col.name) ?? "";
            fields.push({
                name: col.name,
                value: stringToValue(strValue, col.sql_type),
            });
        }

        // For updates, pass the dirty fields set
        onSave({ fields }, row ? dirtyFields : undefined);
    }

    function handleDelete() {
        if (onDelete && confirm("Are you sure you want to delete this row?")) {
            onDelete();
        }
    }

    function getFormValue(colName: string): string {
        return formValues.get(colName) ?? "";
    }

    function setFormValue(colName: string, value: string) {
        formValues = new Map(formValues).set(colName, value);
        // Clear validation error when user starts typing
        if (validationErrors.has(colName)) {
            validationErrors = new Map(validationErrors);
            validationErrors.delete(colName);
        }
    }

    function revertField(colName: string) {
        const original = originalValues.get(colName) ?? "";
        formValues = new Map(formValues).set(colName, original);
    }

    function isPrimaryKey(col: ColumnInfo): boolean {
        return col.primary_key;
    }

    // Check if a column is required for inserts (non-nullable, no default, not auto-generated)
    function isRequired(col: ColumnInfo): boolean {
        if (col.auto_generated) return false;
        return !col.nullable && !col.default;
    }

    // Check if a field is dirty (modified)
    function isDirty(colName: string): boolean {
        return dirtyFields.has(colName);
    }

    // Get count of dirty fields
    let dirtyCount = $derived(dirtyFields.size);

    // Get missing required fields for insert validation
    let missingRequired = $derived.by(() => {
        if (row) return []; // Not creating
        return columns.filter((col) => {
            if (!isRequired(col)) return false;
            const value = formValues.get(col.name) ?? "";
            return value === "";
        });
    });

    // Determine what type of control to use for a column
    function getControlType(
        col: ColumnInfo,
    ): "checkbox" | "number" | "datetime" | "textarea" | "text" | "enum" | "codemirror" {
        const typeLower = col.sql_type.toLowerCase();

        // Fields with lang get CodeMirror
        if (col.lang) {
            return "codemirror";
        }

        // Enum columns with variants get a dropdown
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

        // Long text fields or JSON get textarea
        if (col.long || typeLower.includes("json")) {
            return "textarea";
        }

        return "text";
    }

    function getBooleanValue(colName: string): boolean {
        const val = getFormValue(colName);
        return val.toLowerCase() === "true" || val === "1";
    }

    function setBooleanValue(colName: string, checked: boolean) {
        setFormValue(colName, checked ? "true" : "false");
    }

    // FK support
    function getFkInfo(col: ColumnInfo): { fkTable: TableInfo } | null {
        if (!table || !schema) return null;
        const fk = getFkForColumn(table, col.name);
        if (!fk) return null;
        const targetTable = getTableByName(schema, fk.references_table);
        if (!targetTable) return null;
        return { fkTable: targetTable };
    }

    // Get icon for a language
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
            case "css":
                return "palette";
            case "javascript":
            case "js":
                return "file-json";
            case "typescript":
            case "ts":
                return "file-type";
            default:
                return "code";
        }
    }
</script>

{#snippet formFields()}
    {#each columns as col}
        {@const isAuto = col.auto_generated}
        {@const isPK = isPrimaryKey(col)}
        {@const hideOnCreate = !row && isAuto}
        {@const disabled = isAuto || (row !== null && isPK)}
        {@const controlType = getControlType(col)}
        {@const fkInfo = getFkInfo(col)}

        {#if hideOnCreate}
            <!-- Skip auto-generated fields when creating -->
        {:else}
            {@const required = !row && isRequired(col)}
            {@const dirty = row && isDirty(col.name)}
            {@const hasError = validationErrors.has(col.name)}
            {@const tooltipContent = [col.sql_type, isPK ? "primary key" : null, col.doc]
                .filter(Boolean)
                .join(" · ")}
            {@const langIcon = getLangIcon(col.lang)}

            <div class="field-group">
                <div class="field-label-row">
                    {#if langIcon}
                        <DynamicIcon name={langIcon} size={14} class="field-icon" />
                    {:else if col.icon}
                        <DynamicIcon name={col.icon} size={14} class="field-icon" />
                    {/if}
                    <Label for={col.name}>{col.doc || col.name}</Label>
                    {#if required}
                        <Asterisk size={10} class="required-indicator" weight="bold" />
                    {/if}
                    {#if dirty}
                        <span class="modified-indicator">modified</span>
                        <button
                            type="button"
                            class="revert-btn"
                            onclick={() => revertField(col.name)}
                            title="Revert to original value"
                        >
                            <ArrowCounterClockwise size={12} />
                        </button>
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
                {#if hasError}
                    <p class="error-text">{validationErrors.get(col.name)}</p>
                {/if}

                {#if controlType === "checkbox"}
                    <div class="checkbox-row">
                        <Checkbox
                            id={col.name}
                            checked={getBooleanValue(col.name)}
                            onCheckedChange={(checked) =>
                                setBooleanValue(col.name, checked === true)}
                            {disabled}
                        />
                        <span class="checkbox-value">
                            {getBooleanValue(col.name) ? "true" : "false"}
                        </span>
                    </div>
                {:else if controlType === "number"}
                    {#if fkInfo && client && !disabled}
                        <div class="fk-row">
                            <div class="fk-select">
                                <FkSelect
                                    value={getFormValue(col.name)}
                                    fkTable={fkInfo.fkTable}
                                    {client}
                                    {disabled}
                                    onchange={(v) => setFormValue(col.name, v)}
                                />
                            </div>
                            <span class="fk-indicator">
                                <ArrowSquareOut size={12} />
                                {fkInfo.fkTable.name}
                            </span>
                        </div>
                    {:else}
                        <NumberInput
                            id={col.name}
                            value={getFormValue(col.name)}
                            oninput={(e) => setFormValue(col.name, e.currentTarget.value)}
                            placeholder={col.nullable ? "null" : ""}
                            {disabled}
                        />
                    {/if}
                {:else if controlType === "datetime"}
                    <DatetimeInput
                        id={col.name}
                        value={getFormValue(col.name)}
                        onchange={(v: string) => setFormValue(col.name, v)}
                        {disabled}
                    />
                {:else if controlType === "textarea"}
                    <Textarea
                        id={col.name}
                        value={getFormValue(col.name)}
                        oninput={(e) => setFormValue(col.name, e.currentTarget.value)}
                        placeholder={col.nullable ? "null" : ""}
                        disabled={disabled || false}
                        rows={3}
                    />
                {:else if controlType === "enum"}
                    <Select.Root
                        type="single"
                        value={getFormValue(col.name)}
                        {disabled}
                        onValueChange={(v: string) => setFormValue(col.name, v)}
                    >
                        <Select.Trigger class="full-width">
                            {getFormValue(col.name) || "-- None --"}
                        </Select.Trigger>
                        <Select.Content>
                            {#if col.nullable}
                                <Select.Item value="">-- None --</Select.Item>
                            {/if}
                            {#each col.enum_variants as variant}
                                <Select.Item value={variant}>{variant}</Select.Item>
                            {/each}
                        </Select.Content>
                    </Select.Root>
                {:else if controlType === "codemirror"}
                    <CodeMirrorEditor
                        value={getFormValue(col.name)}
                        lang={col.lang}
                        {disabled}
                        placeholder={col.nullable ? "null" : ""}
                        onchange={(v) => setFormValue(col.name, v)}
                    />
                {:else}
                    <Input
                        id={col.name}
                        type="text"
                        value={getFormValue(col.name)}
                        oninput={(e) => setFormValue(col.name, e.currentTarget.value)}
                        placeholder={col.nullable ? "null" : ""}
                        {disabled}
                    />
                {/if}
            </div>
        {/if}
    {/each}
{/snippet}

{#snippet footer()}
    {#if row && onDelete}
        <div class="footer-left">
            <Button variant="destructive" onclick={handleDelete} disabled={deleting}>
                <Trash size={16} />
                {deleting ? "Deleting..." : "Delete"}
            </Button>
        </div>
    {/if}

    <!-- Status indicator -->
    <div class="status-text">
        {#if row}
            {#if dirtyCount > 0}
                {dirtyCount} field{dirtyCount === 1 ? "" : "s"} modified
            {:else}
                No changes
            {/if}
        {:else if missingRequired.length > 0}
            <span class="error-status">
                {missingRequired.length} required field{missingRequired.length === 1 ? "" : "s"} missing
            </span>
        {:else}
            Ready to create
        {/if}
    </div>

    <Button variant="outline" onclick={onClose} disabled={saving || deleting}>Cancel</Button>
    <Button
        onclick={handleSave}
        disabled={saving || deleting || (row !== null && dirtyCount === 0)}
    >
        {#if saving}
            Saving...
        {:else if row}
            Update{dirtyCount > 0 ? ` (${dirtyCount})` : ""}
        {:else}
            Create
        {/if}
    </Button>
{/snippet}

{#if fullscreen}
    <!-- Full-screen panel mode -->
    <div class="fullscreen-panel">
        <!-- Header with back button -->
        <header class="panel-header">
            <Button variant="ghost" size="icon" onclick={onClose}>
                <ArrowLeft size={20} />
            </Button>
            <div>
                <h1 class="panel-title">
                    {row ? "Edit" : "New"}
                    {tableName}
                </h1>
            </div>
        </header>

        <!-- Scrollable form content -->
        <div class="panel-content">
            <div class="form-fields">
                {@render formFields()}
            </div>

            <!-- Related tables section (only when viewing existing row) -->
            {#if row && table && schema && client}
                <div class="related-tables">
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

        <!-- Footer with actions -->
        <footer class="panel-footer">
            {@render footer()}
        </footer>
    </div>
{:else}
    <!-- Dialog mode (original) -->
    <Dialog.Root open={true} onOpenChange={(open) => !open && onClose()}>
        <Dialog.Content>
            <Dialog.Header>
                <Dialog.Title>{row ? "Edit Row" : "Create Row"}</Dialog.Title>
            </Dialog.Header>

            <div class="form-fields">
                {@render formFields()}
            </div>

            <Dialog.Footer>
                {@render footer()}
            </Dialog.Footer>
        </Dialog.Content>
    </Dialog.Root>
{/if}

<style>
    .field-group {
        margin-bottom: 1.25rem;
    }

    .field-label-row {
        display: flex;
        align-items: center;
        gap: 0.375rem;
        margin-bottom: 0.5rem;
    }

    :global(.field-icon) {
        color: var(--muted-foreground);
        opacity: 0.6;
    }

    :global(.required-indicator) {
        color: var(--destructive);
    }

    .modified-indicator {
        font-size: 0.625rem;
        color: var(--chart-4);
        font-weight: 500;
    }

    .revert-btn {
        background: none;
        border: none;
        padding: 0;
        cursor: pointer;
        color: var(--muted-foreground);
        opacity: 0.6;
        transition:
            color 0.15s,
            opacity 0.15s;
    }

    .revert-btn:hover {
        color: var(--foreground);
        opacity: 1;
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

    .error-text {
        font-size: 0.75rem;
        color: var(--destructive);
        margin: 0.25rem 0;
    }

    .checkbox-row {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        height: 2.25rem;
    }

    .checkbox-value {
        font-size: 0.875rem;
        color: var(--muted-foreground);
    }

    .fk-row {
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .fk-select {
        flex: 1;
    }

    .fk-indicator {
        font-size: 0.75rem;
        color: var(--muted-foreground);
        display: flex;
        align-items: center;
        gap: 0.25rem;
    }

    :global(.full-width) {
        width: 100%;
    }

    .form-fields {
        display: flex;
        flex-direction: column;
        gap: 1rem;
    }

    .footer-left {
        margin-right: auto;
    }

    .status-text {
        flex: 1;
        font-size: 0.75rem;
        color: var(--muted-foreground);
    }

    .error-status {
        color: var(--destructive);
    }

    /* Fullscreen panel styles */
    .fullscreen-panel {
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
        gap: 1rem;
        padding: 1rem 1.5rem;
        border-bottom: 1px solid var(--border);
        flex-shrink: 0;
    }

    @media (min-width: 768px) {
        .panel-header {
            padding: 1rem 2rem;
        }
    }

    .panel-title {
        font-size: 1.125rem;
        font-weight: 500;
        color: var(--foreground);
        text-transform: uppercase;
        letter-spacing: 0.05em;
        margin: 0;
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

    .related-tables {
        margin-top: 2rem;
    }

    .panel-footer {
        display: flex;
        align-items: center;
        gap: 1rem;
        padding: 1rem 1.5rem;
        border-top: 1px solid var(--border);
        flex-shrink: 0;
    }

    @media (min-width: 768px) {
        .panel-footer {
            padding: 1rem 2rem;
        }
    }
</style>
