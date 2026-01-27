<script lang="ts">
    import type { Snippet } from "svelte";

    interface Props {
        class?: string;
        children?: Snippet;
    }

    let { class: className = "", children }: Props = $props();
</script>

<div class="card-footer {className}">
    {@render children?.()}
</div>

<style>
    .card-footer {
        display: flex;
        align-items: center;
        padding: 1.5rem;
        padding-top: 0;
    }
</style>
