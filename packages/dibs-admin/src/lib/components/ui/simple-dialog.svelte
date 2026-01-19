<script lang="ts">
    import { cn } from "../../utils.js";
    import * as Dialog from "./dialog/index.js";
    import type { Snippet } from "svelte";

    interface Props {
        open: boolean;
        onClose: () => void;
        title: string;
        children: Snippet;
        footer?: Snippet;
        class?: string;
    }

    let { open, onClose, title, children, footer, class: className }: Props = $props();
</script>

<Dialog.Root bind:open onOpenChange={(o) => !o && onClose()}>
    <Dialog.Portal>
        <Dialog.Overlay />
        <Dialog.Content class={cn("sm:max-w-lg max-h-[85vh] overflow-y-auto", className)}>
            <Dialog.Header>
                <Dialog.Title>{title}</Dialog.Title>
            </Dialog.Header>
            <div class="py-4">
                {@render children()}
            </div>
            {#if footer}
                <Dialog.Footer>
                    {@render footer()}
                </Dialog.Footer>
            {/if}
        </Dialog.Content>
    </Dialog.Portal>
</Dialog.Root>
