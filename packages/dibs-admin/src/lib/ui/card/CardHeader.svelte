<script lang="ts">
    import type { Snippet } from "svelte";

    interface Props {
        class?: string;
        children?: Snippet;
    }

    let { class: className = "", children }: Props = $props();
</script>

<div class="card-header {className}">
    {@render children?.()}
</div>

<style>
    .card-header {
        display: flex;
        flex-direction: column;
        gap: 0.375rem;
        padding: 1.5rem;
    }

    /* When followed by content, reduce bottom padding */
    .card-header:has(+ .card-content) {
        padding-bottom: 0.75rem;
    }
</style>
