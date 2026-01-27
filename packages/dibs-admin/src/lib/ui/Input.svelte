<script lang="ts">
    import type { HTMLInputAttributes } from "svelte/elements";

    interface Props extends HTMLInputAttributes {
        class?: string;
        value?: string;
    }

    let {
        class: className = "",
        type = "text",
        value = $bindable(""),
        ...restProps
    }: Props = $props();
</script>

<input {type} bind:value class="input {className}" {...restProps} />

<style>
    .input {
        display: flex;
        width: 100%;
        height: 2.25rem;
        padding: 0.5rem 0.75rem;
        border-radius: var(--radius-md, 0.375rem);
        border: 1px solid var(--border);
        background-color: var(--input);
        font-size: 0.875rem;
        line-height: 1.25rem;
        color: var(--foreground);
        transition:
            border-color 0.15s,
            background-color 0.15s;
    }

    .input::placeholder {
        color: var(--muted-foreground);
    }

    .input:focus {
        outline: none;
        border-color: var(--ring);
        box-shadow: 0 0 0 1px var(--ring);
    }

    .input:disabled {
        cursor: not-allowed;
        opacity: 0.5;
    }

    .input::file-selector-button {
        border: 0;
        background: transparent;
        font-size: 0.875rem;
        font-weight: 500;
        color: var(--foreground);
    }
</style>
