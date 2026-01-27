<script lang="ts">
    import type { Snippet } from "svelte";

    interface Props {
        class?: string;
        children?: Snippet;
    }

    let { class: className = "", children }: Props = $props();
</script>

<div class="card-content {className}">
    {@render children?.()}
</div>

<style>
    .card-content {
        padding: 1.5rem;
    }
</style>
