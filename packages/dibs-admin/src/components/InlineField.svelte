<script lang="ts">
    import { Pencil } from "phosphor-svelte";
    import { Input, NumberInput, Checkbox, Textarea, Select, DatetimeInput } from "../lib/ui/index";
    import CodeMirrorEditor from "./CodeMirrorEditor.svelte";

    type FieldType =
        | "text"
        | "number"
        | "boolean"
        | "datetime"
        | "enum"
        | "textarea"
        | "codemirror";

    interface Props {
        value: string;
        type?: FieldType;
        readOnly?: boolean;
        disabled?: boolean;
        placeholder?: string;
        enumOptions?: string[];
        lang?: string | null;
        /** Called when value changes - parent tracks pending changes */
        onchange?: (newValue: string) => void;
    }

    let {
        value,
        type = "text",
        readOnly = false,
        disabled = false,
        placeholder = "",
        enumOptions = [],
        lang = null,
        onchange,
    }: Props = $props();

    let isEditing = $state(false);
    let editValue = $state("");

    // Keep editValue in sync with value when not editing
    $effect(() => {
        if (!isEditing) {
            editValue = value;
        }
    });

    function startEdit() {
        if (readOnly || disabled) return;
        editValue = value;
        isEditing = true;
    }

    function commitChange() {
        if (editValue !== value) {
            onchange?.(editValue);
        }
        isEditing = false;
    }

    function cancel() {
        editValue = value;
        isEditing = false;
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Enter" && type !== "textarea" && type !== "codemirror") {
            e.preventDefault();
            commitChange();
        } else if (e.key === "Escape") {
            e.preventDefault();
            cancel();
        }
    }

    function handleBlur() {
        // Commit on blur for simple fields
        if (type === "textarea" || type === "codemirror") return;
        if (isEditing) {
            commitChange();
        }
    }

    function formatDisplayValue(val: string): string {
        if (val === "" || val === "null") return placeholder || "—";
        if (type === "boolean") return val === "true" ? "Yes" : "No";
        if (type === "datetime") {
            const date = new Date(val);
            if (!isNaN(date.getTime())) {
                return date.toLocaleString();
            }
        }
        // For textarea/codemirror, show more content (up to ~500 chars / 8 lines)
        if (type === "textarea" || type === "codemirror") {
            if (val.length > 500) return val.slice(0, 500) + "…";
            return val;
        }
        // Truncate long values for display
        if (val.length > 100) return val.slice(0, 100) + "…";
        return val;
    }

    function getBoolValue(): boolean {
        return editValue.toLowerCase() === "true" || editValue === "1";
    }

    function setBoolValue(checked: boolean) {
        editValue = checked ? "true" : "false";
        // For boolean, commit immediately but don't "save" - just report change
        onchange?.(editValue);
        isEditing = false;
    }

    function handleEnumChange(v: string) {
        editValue = v;
        onchange?.(editValue);
        isEditing = false;
    }

    function handleDatetimeChange(v: string) {
        editValue = v;
        onchange?.(editValue);
        isEditing = false;
    }
</script>

{#if type === "boolean"}
    <!-- Boolean: just a checkbox, no wrapper needed -->
    <Checkbox
        checked={value === "true" || value === "1"}
        onCheckedChange={(checked) => {
            if (!readOnly && !disabled) {
                onchange?.(checked ? "true" : "false");
            }
        }}
        disabled={readOnly || disabled}
    />
{:else}
    <div class="inline-field" class:editing={isEditing}>
        {#if isEditing}
            <div class="edit-container">
                {#if type === "number"}
                    <Input
                        type="number"
                        value={editValue}
                        oninput={(e) => (editValue = e.currentTarget.value)}
                        onkeydown={handleKeydown}
                        onblur={handleBlur}
                        {placeholder}
                        {disabled}
                        class="edit-input"
                    />
                {:else if type === "datetime"}
                    <DatetimeInput value={editValue} onchange={handleDatetimeChange} {disabled} />
                {:else if type === "enum"}
                    <Select.Root
                        type="single"
                        value={editValue}
                        {disabled}
                        onValueChange={handleEnumChange}
                    >
                        <Select.Trigger class="full-width">
                            {editValue || placeholder || "— Select —"}
                        </Select.Trigger>
                        <Select.Content>
                            <Select.Item value="">— None —</Select.Item>
                            {#each enumOptions as option}
                                <Select.Item value={option}>{option}</Select.Item>
                            {/each}
                        </Select.Content>
                    </Select.Root>
                {:else if type === "textarea"}
                    <div class="textarea-container">
                        <Textarea
                            value={editValue}
                            oninput={(e) => (editValue = e.currentTarget.value)}
                            onkeydown={handleKeydown}
                            onblur={() => commitChange()}
                            {placeholder}
                            disabled={disabled || false}
                            rows={4}
                        />
                    </div>
                {:else if type === "codemirror"}
                    <div class="codemirror-container">
                        <CodeMirrorEditor
                            value={editValue}
                            {lang}
                            {disabled}
                            {placeholder}
                            onchange={(v) => {
                                editValue = v;
                                // For codemirror, commit changes as user types
                                onchange?.(v);
                            }}
                        />
                    </div>
                {:else}
                    <Input
                        type="text"
                        value={editValue}
                        oninput={(e) => (editValue = e.currentTarget.value)}
                        onkeydown={handleKeydown}
                        onblur={handleBlur}
                        {placeholder}
                        {disabled}
                        class="edit-input"
                    />
                {/if}
            </div>
        {:else}
            <!-- Display mode -->
            <button
                type="button"
                class="display-value"
                class:readonly={readOnly || disabled}
                class:multiline={type === "textarea" || type === "codemirror"}
                onclick={startEdit}
                disabled={readOnly || disabled}
            >
                <span class:empty={value === "" || value === "null"}>
                    {formatDisplayValue(value)}
                </span>
            </button>
            {#if !readOnly && !disabled}
                <span
                    class="edit-icon"
                    class:multiline-icon={type === "textarea" || type === "codemirror"}
                >
                    <Pencil size={14} />
                </span>
            {/if}
        {/if}
    </div>
{/if}

<style>
    /* Container - block element, children are block by default */
    .inline-field {
        position: relative;
    }

    /* Edit mode container */
    .edit-container {
        display: block;
    }

    /* Boolean field - always visible, styled like other fields */
    .bool-field {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        height: 2.25rem;
        padding: 0 0.75rem;
        background-color: var(--input);
        border-radius: 0.375rem;
        border: 1px solid var(--border);
        width: 100%;
        box-sizing: border-box;
    }

    .bool-field.readonly {
        background-color: oklch(from var(--input) l c h / 0.5);
        cursor: default;
    }

    .bool-label {
        font-size: 0.875rem;
        font-weight: 500;
    }

    .bool-field.readonly .bool-label {
        color: var(--muted-foreground);
        font-weight: 400;
    }

    /* Textarea and codemirror containers */
    .textarea-container,
    .codemirror-container {
        display: block;
    }

    /* Display value - the main clickable field */
    .display-value {
        display: block;
        width: 100%;
        box-sizing: border-box;
        text-align: left;
        padding: 0.5rem 0.75rem;
        min-height: 2.25rem;
        border-radius: 0.375rem;
        font-size: 0.875rem;
        font-weight: 500;
        border: 1px solid var(--border);
        background-color: var(--input);
        cursor: pointer;
        color: var(--foreground);
        transition:
            background-color 0.15s,
            border-color 0.15s;
    }

    .display-value:hover:not(.readonly) {
        border-color: var(--ring);
        background-color: var(--accent);
    }

    .display-value:focus {
        outline: none;
        border-color: var(--ring);
    }

    .display-value.readonly {
        background-color: oklch(from var(--input) l c h / 0.5);
        cursor: default;
        font-weight: 400;
        color: var(--muted-foreground);
    }

    .display-value .empty {
        color: var(--muted-foreground);
        font-weight: 400;
        font-style: italic;
    }

    .display-value.multiline {
        white-space: pre-wrap;
        min-height: 6rem;
        padding-top: 0.75rem;
        padding-bottom: 0.75rem;
    }

    /* Edit icon - absolutely positioned */
    .edit-icon {
        position: absolute;
        right: 0.5rem;
        top: 50%;
        transform: translateY(-50%);
        opacity: 0;
        transition: opacity 0.15s;
        color: var(--muted-foreground);
        pointer-events: none;
    }

    .edit-icon.multiline-icon {
        top: 0.75rem;
        transform: none;
    }

    .inline-field:hover .edit-icon {
        opacity: 1;
    }
</style>
