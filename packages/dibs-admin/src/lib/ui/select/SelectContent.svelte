<script lang="ts">
    import { Select as SelectPrimitive } from "bits-ui";
    import type { Snippet } from "svelte";

    interface Props {
        class?: string;
        children?: Snippet;
        sideOffset?: number;
    }

    let { class: className = "", children, sideOffset = 4 }: Props = $props();
</script>

<SelectPrimitive.Portal>
    <SelectPrimitive.Content class="select-content {className}" {sideOffset}>
        <SelectPrimitive.Viewport class="select-viewport">
            {@render children?.()}
        </SelectPrimitive.Viewport>
    </SelectPrimitive.Content>
</SelectPrimitive.Portal>

<style>
    :global(.select-content) {
        z-index: 50;
        min-width: var(--bits-select-trigger-width, 8rem);
        max-height: 20rem;
        overflow: hidden;
        border-radius: var(--radius-md, 0.375rem);
        border: 1px solid var(--border);
        background-color: var(--popover);
        color: var(--popover-foreground);
        box-shadow:
            0 4px 6px -1px rgb(0 0 0 / 0.1),
            0 2px 4px -2px rgb(0 0 0 / 0.1);
    }

    :global(.select-viewport) {
        padding: 0.25rem;
    }
</style>
