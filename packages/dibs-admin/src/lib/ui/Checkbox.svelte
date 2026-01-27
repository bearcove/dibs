<script lang="ts">
    import { Checkbox as CheckboxPrimitive } from "bits-ui";
    import CheckIcon from "phosphor-svelte/lib/CheckIcon";
    import MinusIcon from "phosphor-svelte/lib/MinusIcon";

    interface Props {
        id?: string;
        checked?: boolean;
        disabled?: boolean;
        required?: boolean;
        name?: string;
        value?: string;
        class?: string;
        onCheckedChange?: (checked: boolean) => void;
    }

    let {
        id,
        checked = $bindable(false),
        disabled = false,
        required = false,
        name,
        value,
        class: className = "",
        onCheckedChange,
    }: Props = $props();

    function handleCheckedChange(val: boolean) {
        checked = val;
        onCheckedChange?.(val);
    }
</script>

<CheckboxPrimitive.Root
    {id}
    {checked}
    onCheckedChange={handleCheckedChange}
    {disabled}
    {required}
    {name}
    {value}
    class="checkbox {className}"
>
    {#snippet children({ checked: isChecked, indeterminate })}
        {#if indeterminate}
            <MinusIcon size={12} weight="bold" />
        {:else if isChecked}
            <CheckIcon size={12} weight="bold" />
        {/if}
    {/snippet}
</CheckboxPrimitive.Root>

<style>
    :global(.checkbox) {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 1rem;
        height: 1rem;
        flex-shrink: 0;
        border-radius: var(--radius-sm, 0.25rem);
        border: 1px solid var(--primary);
        background-color: transparent;
        color: var(--primary-foreground);
        transition:
            background-color 0.15s,
            border-color 0.15s;
        cursor: pointer;
    }

    :global(.checkbox:focus-visible) {
        outline: 2px solid var(--ring);
        outline-offset: 2px;
    }

    :global(.checkbox:disabled) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.checkbox[data-state="checked"]),
    :global(.checkbox[data-state="indeterminate"]) {
        background-color: var(--primary);
        border-color: var(--primary);
    }
</style>
