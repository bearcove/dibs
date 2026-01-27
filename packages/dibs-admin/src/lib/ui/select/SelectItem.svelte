<script lang="ts">
    import { Select as SelectPrimitive } from "bits-ui";
    import type { Snippet } from "svelte";

    interface Props {
        value: string;
        disabled?: boolean;
        class?: string;
        children?: Snippet;
    }

    let { value, disabled = false, class: className = "", children }: Props = $props();
</script>

<SelectPrimitive.Item {value} {disabled} class="select-item {className}">
    {#snippet child({ props, selected })}
        <div {...props} class="select-item {className}" class:selected>
            {@render children?.()}
        </div>
    {/snippet}
</SelectPrimitive.Item>

<style>
    :global(.select-item) {
        display: block;
        padding: 0.5rem 0.75rem;
        font-size: 0.875rem;
        cursor: pointer;
        outline: none;
        user-select: none;
    }

    :global(.select-item:focus),
    :global(.select-item[data-highlighted]) {
        background-color: var(--accent);
        color: var(--accent-foreground);
    }

    :global(.select-item[data-disabled]) {
        pointer-events: none;
        opacity: 0.5;
    }

    :global(.select-item.selected) {
        font-weight: 600;
        background-color: oklch(from var(--accent) l c h / 0.5);
    }
</style>
