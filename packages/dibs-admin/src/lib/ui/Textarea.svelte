<script lang="ts">
    import type { HTMLTextareaAttributes } from "svelte/elements";

    interface Props extends HTMLTextareaAttributes {
        class?: string;
    }

    let { class: className = "", ...restProps }: Props = $props();
</script>

<textarea class="textarea {className}" {...restProps}></textarea>

<style>
    .textarea {
        display: flex;
        width: 100%;
        min-height: 5rem;
        padding: 0.5rem 0.75rem;
        border-radius: var(--radius-md, 0.375rem);
        border: 1px solid var(--input);
        background-color: transparent;
        font-size: 0.875rem;
        line-height: 1.5;
        color: var(--foreground);
        resize: vertical;
        transition:
            border-color 0.15s,
            outline 0.15s;
        font-family: inherit;
    }

    .textarea::placeholder {
        color: var(--muted-foreground);
    }

    .textarea:focus {
        outline: none;
        border-color: var(--ring);
        box-shadow: 0 0 0 1px var(--ring);
    }

    .textarea:disabled {
        cursor: not-allowed;
        opacity: 0.5;
    }
</style>
