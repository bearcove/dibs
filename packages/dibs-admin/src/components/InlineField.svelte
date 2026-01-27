<script lang="ts">
    import { Input, Checkbox, Textarea, Select, DatetimeInput } from "../lib/ui/index";
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

    // For inputs, we track a local value and report changes on blur
    let localValue = $state(value);

    // Sync localValue when value prop changes from outside
    $effect(() => {
        localValue = value;
    });

    function handleInput(e: Event) {
        localValue = (e.currentTarget as HTMLInputElement).value;
    }

    function handleBlur() {
        if (localValue !== value) {
            onchange?.(localValue);
        }
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Enter" && type !== "textarea" && type !== "codemirror") {
            e.preventDefault();
            (e.currentTarget as HTMLInputElement).blur();
        } else if (e.key === "Escape") {
            e.preventDefault();
            localValue = value;
            (e.currentTarget as HTMLInputElement).blur();
        }
    }
</script>

{#if type === "boolean"}
    <Checkbox
        checked={value === "true" || value === "1"}
        onCheckedChange={(checked) => {
            if (!readOnly && !disabled) {
                onchange?.(checked ? "true" : "false");
            }
        }}
        disabled={readOnly || disabled}
    />
{:else if type === "text" || type === "number"}
    <Input
        type={type === "number" ? "number" : "text"}
        value={localValue}
        oninput={handleInput}
        onblur={handleBlur}
        onkeydown={handleKeydown}
        placeholder={placeholder || undefined}
        disabled={readOnly || disabled}
        class="field-input"
    />
{:else if type === "datetime"}
    <DatetimeInput {value} onchange={(v) => onchange?.(v)} disabled={readOnly || disabled} />
{:else if type === "enum"}
    <Select.Root
        type="single"
        {value}
        disabled={readOnly || disabled}
        onValueChange={(v) => onchange?.(v)}
    >
        <Select.Trigger>
            {value || placeholder || "— Select —"}
        </Select.Trigger>
        <Select.Content>
            <Select.Item value="">— None —</Select.Item>
            {#each enumOptions as option}
                <Select.Item value={option}>{option}</Select.Item>
            {/each}
        </Select.Content>
    </Select.Root>
{:else if type === "textarea"}
    <Textarea
        value={localValue}
        oninput={handleInput}
        onblur={handleBlur}
        placeholder={placeholder || undefined}
        disabled={readOnly || disabled}
        rows={4}
    />
{:else if type === "codemirror"}
    <CodeMirrorEditor
        {value}
        {lang}
        disabled={readOnly || disabled}
        {placeholder}
        onchange={(v) => onchange?.(v)}
    />
{/if}

<style>
    :global(.field-input) {
        width: 100%;
    }
</style>
