<script lang="ts">
    import { Dialog as DialogPrimitive } from "bits-ui";
    import XIcon from "phosphor-svelte/lib/XIcon";
    import type { Snippet } from "svelte";

    interface Props {
        class?: string;
        children?: Snippet;
        showCloseButton?: boolean;
    }

    let { class: className = "", children, showCloseButton = true }: Props = $props();
</script>

<DialogPrimitive.Portal>
    <DialogPrimitive.Overlay class="dialog-overlay" />
    <DialogPrimitive.Content class="dialog-content {className}">
        {@render children?.()}
        {#if showCloseButton}
            <DialogPrimitive.Close class="dialog-close">
                <XIcon size={16} />
                <span class="sr-only">Close</span>
            </DialogPrimitive.Close>
        {/if}
    </DialogPrimitive.Content>
</DialogPrimitive.Portal>

<style>
    :global(.dialog-overlay) {
        position: fixed;
        inset: 0;
        z-index: 50;
        background-color: rgb(0 0 0 / 0.5);
    }

    :global(.dialog-overlay[data-state="open"]) {
        animation: fade-in 0.15s ease-out;
    }

    :global(.dialog-overlay[data-state="closed"]) {
        animation: fade-out 0.15s ease-in;
    }

    :global(.dialog-content) {
        position: fixed;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%);
        z-index: 50;
        display: grid;
        width: 100%;
        max-width: calc(100% - 2rem);
        gap: 1rem;
        border-radius: var(--radius-lg, 0.5rem);
        border: 1px solid var(--border);
        background-color: var(--background);
        padding: 1.5rem;
        box-shadow: 0 25px 50px -12px rgb(0 0 0 / 0.25);
    }

    @media (min-width: 640px) {
        :global(.dialog-content) {
            max-width: 32rem;
        }
    }

    :global(.dialog-content[data-state="open"]) {
        animation: dialog-in 0.2s ease-out;
    }

    :global(.dialog-content[data-state="closed"]) {
        animation: dialog-out 0.15s ease-in;
    }

    :global(.dialog-close) {
        position: absolute;
        top: 1rem;
        right: 1rem;
        display: flex;
        align-items: center;
        justify-content: center;
        padding: 0.25rem;
        border-radius: var(--radius-sm, 0.25rem);
        border: none;
        background: transparent;
        color: var(--muted-foreground);
        cursor: pointer;
        opacity: 0.7;
        transition:
            opacity 0.15s,
            background-color 0.15s;
    }

    :global(.dialog-close:hover) {
        opacity: 1;
        background-color: var(--accent);
    }

    :global(.dialog-close:focus) {
        outline: 2px solid var(--ring);
        outline-offset: 2px;
    }

    :global(.sr-only) {
        position: absolute;
        width: 1px;
        height: 1px;
        padding: 0;
        margin: -1px;
        overflow: hidden;
        clip: rect(0, 0, 0, 0);
        white-space: nowrap;
        border-width: 0;
    }

    @keyframes fade-in {
        from {
            opacity: 0;
        }
        to {
            opacity: 1;
        }
    }

    @keyframes fade-out {
        from {
            opacity: 1;
        }
        to {
            opacity: 0;
        }
    }

    @keyframes dialog-in {
        from {
            opacity: 0;
            transform: translate(-50%, -50%) scale(0.95);
        }
        to {
            opacity: 1;
            transform: translate(-50%, -50%) scale(1);
        }
    }

    @keyframes dialog-out {
        from {
            opacity: 1;
            transform: translate(-50%, -50%) scale(1);
        }
        to {
            opacity: 0;
            transform: translate(-50%, -50%) scale(0.95);
        }
    }
</style>
