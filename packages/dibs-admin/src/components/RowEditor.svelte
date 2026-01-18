<script lang="ts">
    import { Trash, ArrowSquareOut, Asterisk } from "phosphor-svelte";
    import type { Row, RowField, ColumnInfo, Value, TableInfo, SchemaInfo, SquelClient } from "../types.js";
    import { Button } from "../lib/components/ui/index.js";
    import { Input } from "../lib/components/ui/index.js";
    import { NumberInput } from "../lib/components/ui/index.js";
    import { DatetimeInput } from "../lib/components/ui/index.js";
    import { Textarea } from "../lib/components/ui/index.js";
    import { Checkbox } from "../lib/components/ui/index.js";
    import { Label } from "../lib/components/ui/index.js";
    import { Dialog } from "../lib/components/ui/index.js";
    import { Select } from "../lib/components/ui/index.js";
    import { getFkForColumn, getTableByName } from "../lib/fk-utils.js";
    import FkSelect from "./FkSelect.svelte";

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
        databaseUrl?: string;
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
        databaseUrl,
    }: Props = $props();

    // Form state - map column name to string value
    let formValues = $state<Map<string, string>>(new Map());

    // Track which fields have been modified (for updates)
    let dirtyFields = $state<Set<string>>(new Set());

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
        // Reset dirty tracking when row changes
        dirtyFields = new Set();
        validationErrors = new Map();
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
        // Mark field as dirty
        dirtyFields = new Set(dirtyFields).add(colName);
        // Clear validation error when user starts typing
        if (validationErrors.has(colName)) {
            validationErrors = new Map(validationErrors);
            validationErrors.delete(colName);
        }
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
        return columns.filter(col => {
            if (!isRequired(col)) return false;
            const value = formValues.get(col.name) ?? "";
            return value === "";
        });
    });

    // Determine what type of control to use for a column
    function getControlType(col: ColumnInfo): "checkbox" | "number" | "datetime" | "textarea" | "text" | "enum" {
        const typeLower = col.sql_type.toLowerCase();

        // Enum columns with variants get a dropdown
        if (col.enum_variants.length > 0) {
            return "enum";
        }

        if (typeLower.includes("bool")) {
            return "checkbox";
        }

        if (
            typeLower.includes("timestamp") ||
            typeLower.includes("datetime")
        ) {
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
</script>

<Dialog open={true} {onClose} title={row ? "Edit Row" : "Create Row"}>
    <div class="space-y-4">
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

            <div class="space-y-1.5">
                <div>
                    <div class="flex items-center gap-2">
                        <Label for={col.name}>{col.name}</Label>
                        {#if required}
                            <Asterisk size={10} class="text-red-400" weight="bold" />
                        {/if}
                        {#if dirty}
                            <span class="text-[10px] text-amber-400">modified</span>
                        {/if}
                        <span class="text-[10px] text-neutral-600">{col.sql_type}</span>
                        {#if isPK}
                            <span class="text-[10px] text-neutral-500">pk</span>
                        {/if}
                    </div>
                    {#if col.doc}
                        <p class="text-xs text-neutral-500">{col.doc}</p>
                    {/if}
                    {#if hasError}
                        <p class="text-xs text-red-400">{validationErrors.get(col.name)}</p>
                    {/if}
                </div>

                {#if controlType === "checkbox"}
                    <div class="flex items-center gap-3 h-9">
                        <Checkbox
                            id={col.name}
                            checked={getBooleanValue(col.name)}
                            onchange={(checked: boolean) => setBooleanValue(col.name, checked)}
                            {disabled}
                        />
                        <span class="text-sm text-neutral-500">
                            {getBooleanValue(col.name) ? "true" : "false"}
                        </span>
                    </div>
                {:else if controlType === "number"}
                    {#if fkInfo && client && databaseUrl && !disabled}
                        <div class="flex items-center gap-2">
                            <div class="flex-1">
                                <FkSelect
                                    value={getFormValue(col.name)}
                                    fkTable={fkInfo.fkTable}
                                    {client}
                                    {databaseUrl}
                                    {disabled}
                                    onchange={(v) => setFormValue(col.name, v)}
                                />
                            </div>
                            <span class="text-xs text-neutral-500 flex items-center gap-1">
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
                    <Select
                        value={getFormValue(col.name)}
                        {disabled}
                        onchange={() => {
                            const el = document.getElementById(col.name) as HTMLSelectElement;
                            if (el) setFormValue(col.name, el.value);
                        }}
                    >
                        {#if col.nullable}
                            <option value="">-- None --</option>
                        {/if}
                        {#each col.enum_variants as variant}
                            <option value={variant}>{variant}</option>
                        {/each}
                    </Select>
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
    </div>

    {#snippet footer()}
        {#if row && onDelete}
            <div class="mr-auto">
                <Button variant="destructive" onclick={handleDelete} disabled={deleting}>
                    <Trash size={16} />
                    {deleting ? "Deleting..." : "Delete"}
                </Button>
            </div>
        {/if}

        <!-- Status indicator -->
        <div class="flex-1 text-xs text-neutral-500">
            {#if row}
                {#if dirtyCount > 0}
                    {dirtyCount} field{dirtyCount === 1 ? "" : "s"} modified
                {:else}
                    No changes
                {/if}
            {:else}
                {#if missingRequired.length > 0}
                    <span class="text-red-400">
                        {missingRequired.length} required field{missingRequired.length === 1 ? "" : "s"} missing
                    </span>
                {:else}
                    Ready to create
                {/if}
            {/if}
        </div>

        <Button variant="outline" onclick={onClose} disabled={saving || deleting}>Cancel</Button>
        <Button onclick={handleSave} disabled={saving || deleting || (row !== null && dirtyCount === 0)}>
            {#if saving}
                Saving...
            {:else if row}
                Update{dirtyCount > 0 ? ` (${dirtyCount})` : ""}
            {:else}
                Create
            {/if}
        </Button>
    {/snippet}
</Dialog>
