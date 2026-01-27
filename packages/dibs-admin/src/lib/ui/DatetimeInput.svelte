<script lang="ts">
    interface Props {
        value?: string;
        id?: string;
        disabled?: boolean;
        class?: string;
        onchange?: (value: string) => void;
    }

    let {
        value = $bindable(""),
        id,
        disabled = false,
        class: className = "",
        onchange,
    }: Props = $props();

    // Convert PostgreSQL timestamp to datetime-local format
    // Input: "2026-01-18T19:13:05.123456+00:00" (RFC 3339) or "2026-01-18 15:31:19.006542+01"
    // Output: "2026-01-18T15:31:19" (for datetime-local input)
    function toDatetimeLocal(pg: string): string {
        if (!pg) return "";
        const cleaned = pg
            .replace(/\.\d+/, "") // remove microseconds/nanoseconds
            .replace(/Z$/, "") // remove Z timezone
            .replace(/[+-]\d{2}:\d{2}$/, "") // remove RFC 3339 timezone like +00:00
            .replace(/[+-]\d{2}$/, "") // remove short timezone like +01
            .replace(" ", "T"); // space to T for ISO
        return cleaned;
    }

    // Convert datetime-local back to a format we can send
    // Input: "2026-01-18T15:31:19"
    // Output: "2026-01-18 15:31:19" (PostgreSQL-friendly)
    function fromDatetimeLocal(dt: string): string {
        if (!dt) return "";
        return dt.replace("T", " ");
    }

    let localValue = $derived(toDatetimeLocal(value));

    function handleInput(e: Event) {
        const target = e.target as HTMLInputElement;
        const newValue = fromDatetimeLocal(target.value);
        value = newValue;
        onchange?.(newValue);
    }
</script>

<input
    type="datetime-local"
    {id}
    value={localValue}
    oninput={handleInput}
    {disabled}
    step="1"
    class="datetime-input {className}"
/>

<style>
    .datetime-input {
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
    }

    .datetime-input:focus {
        outline: none;
        border-color: var(--ring);
        box-shadow: 0 0 0 1px var(--ring);
    }

    .datetime-input:disabled {
        cursor: not-allowed;
        opacity: 0.5;
        background-color: var(--muted);
    }
</style>
