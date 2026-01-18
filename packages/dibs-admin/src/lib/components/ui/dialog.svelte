<script lang="ts">
    import { cn } from "../../utils.js";
    import { X } from "phosphor-svelte";

    interface Props {
        open: boolean;
        onClose: () => void;
        title?: string;
        class?: string;
        children?: import("svelte").Snippet;
        footer?: import("svelte").Snippet;
    }

    let { open, onClose, title, class: className, children, footer }: Props = $props();

    function handleBackdropClick(e: MouseEvent) {
        if (e.target === e.currentTarget) {
            onClose();
        }
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Escape") {
            onClose();
        }
    }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
    <div
        class="fixed inset-0 z-50 bg-black/80 flex items-center justify-center p-8"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
        onclick={handleBackdropClick}
    >
        <div
            class={cn(
                "bg-neutral-950 w-full max-w-lg max-h-[85vh] flex flex-col",
                className,
            )}
        >
            {#if title}
                <div class="flex items-center justify-between px-6 py-5">
                    <h3 class="text-base font-medium text-white uppercase tracking-wide">{title}</h3>
                    <button
                        type="button"
                        class="text-neutral-500 hover:text-white transition-colors"
                        onclick={onClose}
                    >
                        <X size={20} />
                    </button>
                </div>
            {/if}

            <div class="flex-1 overflow-y-auto px-6 py-4">
                {@render children?.()}
            </div>

            {#if footer}
                <div class="px-6 py-5 flex justify-end gap-4">
                    {@render footer?.()}
                </div>
            {/if}
        </div>
    </div>
{/if}
