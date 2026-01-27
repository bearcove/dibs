<script lang="ts">
    import type { HTMLInputAttributes } from "svelte/elements";

    interface Props extends Omit<HTMLInputAttributes, "type"> {
        class?: string;
    }

    let { class: className = "", ...restProps }: Props = $props();
</script>

<input type="number" class="number-input {className}" {...restProps} />

<style>
    .number-input {
        display: flex;
        width: 100%;
        height: 2.25rem;
        padding: 0.25rem 0.75rem;
        border-radius: var(--radius-md, 0.375rem);
        border: 1px solid var(--input);
        background-color: transparent;
        font-size: 0.875rem;
        color: var(--foreground);
        transition:
            border-color 0.15s,
            outline 0.15s;
        /* Hide spin buttons */
        -moz-appearance: textfield;
    }

    .number-input::-webkit-outer-spin-button,
    .number-input::-webkit-inner-spin-button {
        -webkit-appearance: none;
        margin: 0;
    }

    .number-input::placeholder {
        color: var(--muted-foreground);
    }

    .number-input:focus {
        outline: none;
        border-color: var(--ring);
        box-shadow: 0 0 0 1px var(--ring);
    }

    .number-input:disabled {
        cursor: not-allowed;
        opacity: 0.5;
        background-color: var(--muted);
    }
</style>
