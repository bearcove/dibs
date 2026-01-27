<script lang="ts">
    import { Pencil } from "phosphor-svelte";
    import {
        Input,
        NumberInput,
        Checkbox,
        Textarea,
        Select,
        DatetimeInput,
    } from "../lib/ui/index";
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

<div class="inline-field" class:editing={isEditing}>
    {#if isEditing}
        <div class="edit-container">
            {#if type === "boolean"}
                <div class="bool-edit">
                    <Checkbox
                        checked={getBoolValue()}
                        onCheckedChange={(checked) => setBoolValue(checked === true)}
                        {disabled}
                    />
                    <span class="bool-label">{getBoolValue() ? "Yes" : "No"}</span>
                </div>
            {:else if type === "number"}
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
            onclick={startEdit}
            disabled={readOnly || disabled}
        >
            <span class:empty={value === "" || value === "null"}>
                {formatDisplayValue(value)}
            </span>
        </button>
        {#if !readOnly && !disabled}
            <span class="edit-icon">
                <Pencil size={14} />
            </span>
        {/if}
    {/if}
</div>

<style>
    .inline-field {
        min-height: 2.25rem;
        display: flex;
        align-items: center;
    }

    .edit-container {
        flex: 1;
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .bool-edit {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        height: 2.25rem;
        padding: 0 0.75rem;
    }

    .bool-label {
        font-size: 0.875rem;
    }

    :global(.edit-input) {
        flex: 1;
    }

    :global(.full-width) {
        width: 100%;
    }

    .textarea-container,
    .codemirror-container {
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }

    .display-value {
        flex: 1;
        text-align: left;
        padding: 0.5rem 0.75rem;
        min-height: 2.25rem;
        border-radius: var(--radius-md, 0.375rem);
        font-size: 0.875rem;
        transition: background-color 0.15s;
        border: none;
        background: transparent;
        cursor: pointer;
        color: var(--foreground);
    }

    .display-value:hover:not(.readonly) {
        background-color: oklch(from var(--accent) l c h / 0.5);
    }

    .inline-field:hover .display-value:not(.readonly) {
        background-color: oklch(from var(--accent) l c h / 0.3);
    }

    .display-value.readonly {
        color: var(--muted-foreground);
        cursor: default;
    }

    .display-value .empty {
        color: oklch(from var(--muted-foreground) l c h / 0.6);
    }

    .edit-icon {
        opacity: 0;
        transition: opacity 0.15s;
        color: oklch(from var(--muted-foreground) l c h / 0.6);
        padding-right: 0.5rem;
    }

    .inline-field:hover .edit-icon {
        opacity: 1;
    }
</style>
