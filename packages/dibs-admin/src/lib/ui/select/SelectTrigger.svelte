<script lang="ts">
    import { Select as SelectPrimitive } from "bits-ui";
    import CaretDownIcon from "phosphor-svelte/lib/CaretDownIcon";
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
            <CaretDownIcon size={14} class="select-icon" />
        </button>
    {/snippet}
</SelectPrimitive.Trigger>

<style>
    :global(.select-trigger) {
        display: block;
        position: relative;
        width: 100%;
        box-sizing: border-box;
        height: 2.25rem;
        padding: 0.5rem 2rem 0.5rem 0.75rem;
        border-radius: 0.375rem;
        border: 1px solid var(--border);
        background-color: var(--input);
        font-size: 0.875rem;
        font-weight: 500;
        color: var(--foreground);
        cursor: pointer;
        text-align: left;
        transition:
            border-color 0.15s,
            background-color 0.15s;
    }

    :global(.select-trigger:hover) {
        border-color: var(--ring);
        background-color: var(--accent);
    }

    :global(.select-trigger:focus) {
        outline: none;
        border-color: var(--ring);
    }

    :global(.select-trigger:disabled) {
        cursor: not-allowed;
        opacity: 0.5;
    }

    :global(.select-trigger[data-placeholder]) {
        color: var(--muted-foreground);
        font-weight: 400;
    }

    :global(.select-icon) {
        position: absolute;
        right: 0.75rem;
        top: 50%;
        transform: translateY(-50%);
        color: var(--muted-foreground);
        pointer-events: none;
    }
</style>
