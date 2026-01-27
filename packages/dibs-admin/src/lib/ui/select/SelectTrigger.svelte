<script lang="ts">
    import { Select as SelectPrimitive } from "bits-ui";
    import { CaretDown } from "phosphor-svelte";
    import type { Snippet } from "svelte";

    interface Props {
        class?: string;
        children?: Snippet;
    }

    let { class: className = "", children }: Props = $props();
</script>

<SelectPrimitive.Trigger class="select-trigger {className}">
    {#snippet child({ props })}
        <button {...props} class="select-trigger {className}">
            {@render children?.()}
            <CaretDown size={14} class="select-icon" />
        </button>
    {/snippet}
</SelectPrimitive.Trigger>

<style>
    :global(.select-trigger) {
        display: inline-flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.5rem;
        height: 2.25rem;
        padding: 0.5rem 0.75rem;
        border-radius: var(--radius-md, 0.375rem);
        border: 1px solid var(--input);
        background-color: transparent;
        font-size: 0.875rem;
        color: var(--foreground);
        cursor: pointer;
        transition:
            border-color 0.15s,
            outline 0.15s;
        min-width: 8rem;
    }

    :global(.select-trigger:focus) {
        outline: none;
        border-color: var(--ring);
        box-shadow: 0 0 0 1px var(--ring);
    }

    :global(.select-trigger:disabled) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.select-trigger[data-placeholder]) {
        color: var(--muted-foreground);
    }

    :global(.select-icon) {
        flex-shrink: 0;
        color: var(--muted-foreground);
    }
</style>
