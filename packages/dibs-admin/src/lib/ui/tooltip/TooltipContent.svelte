<script lang="ts">
    import { Tooltip as TooltipPrimitive } from "bits-ui";
    import type { Snippet } from "svelte";

    interface Props {
        class?: string;
        children?: Snippet;
        sideOffset?: number;
    }

    let { class: className = "", children, sideOffset = 4 }: Props = $props();
</script>

<TooltipPrimitive.Portal>
    <TooltipPrimitive.Content class="tooltip-content {className}" {sideOffset}>
        {@render children?.()}
    </TooltipPrimitive.Content>
</TooltipPrimitive.Portal>

<style>
    :global(.tooltip-content) {
        z-index: 50;
        overflow: hidden;
        border-radius: var(--radius-md, 0.375rem);
        border: 1px solid var(--border);
        background-color: var(--popover);
        color: var(--popover-foreground);
        padding: 0.375rem 0.75rem;
        font-size: 0.75rem;
        box-shadow:
            0 4px 6px -1px rgb(0 0 0 / 0.1),
            0 2px 4px -2px rgb(0 0 0 / 0.1);
    }

    :global(.tooltip-content[data-state="delayed-open"]) {
        animation: tooltip-in 0.15s ease-out;
    }

    :global(.tooltip-content[data-state="closed"]) {
        animation: tooltip-out 0.1s ease-in;
    }

    @keyframes tooltip-in {
        from {
            opacity: 0;
            transform: scale(0.95);
        }
        to {
            opacity: 1;
            transform: scale(1);
        }
    }

    @keyframes tooltip-out {
        from {
            opacity: 1;
            transform: scale(1);
        }
        to {
            opacity: 0;
            transform: scale(0.95);
        }
    }
</style>
