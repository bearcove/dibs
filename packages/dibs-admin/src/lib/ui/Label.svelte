<script lang="ts">
    import type { HTMLLabelAttributes } from "svelte/elements";
    import type { Snippet } from "svelte";

    interface Props extends HTMLLabelAttributes {
        class?: string;
        children?: Snippet;
    }

    let { class: className = "", children, ...restProps }: Props = $props();
</script>

<label class="label {className}" {...restProps}>
    {@render children?.()}
</label>

<style>
    .label {
        font-size: 0.75rem;
        font-weight: 500;
        line-height: 1.2;
        color: var(--muted-foreground);
    }

    .label:has(+ :disabled),
    .label:has(+ *:disabled) {
        cursor: not-allowed;
        opacity: 0.7;
    }
</style>
