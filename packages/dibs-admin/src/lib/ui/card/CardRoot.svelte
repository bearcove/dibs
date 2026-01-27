<script lang="ts">
    import type { Snippet } from "svelte";
    import type { HTMLAttributes } from "svelte/elements";

    interface Props extends HTMLAttributes<HTMLDivElement> {
        class?: string;
        children?: Snippet;
    }

    let { class: className = "", children, ...restProps }: Props = $props();
</script>

<div class="card {className}" {...restProps}>
    {@render children?.()}
</div>

<style>
    .card {
        border-radius: var(--radius-lg, 0.5rem);
        border: 1px solid var(--border);
        background-color: var(--card);
        color: var(--card-foreground);
    }
</style>
