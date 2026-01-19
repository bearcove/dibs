<script lang="ts">
    import { cn } from "$lib/utils.js";

    interface Props {
        value?: string;
        id?: string;
        disabled?: boolean;
        class?: string;
        onchange?: (value: string) => void;
    }

    let { value = $bindable(""), id, disabled = false, class: className, onchange }: Props = $props();

    // Convert PostgreSQL timestamp to datetime-local format
    // Input: "2026-01-18T19:13:05.123456+00:00" (RFC 3339) or "2026-01-18 15:31:19.006542+01"
    // Output: "2026-01-18T15:31:19" (for datetime-local input)
    function toDatetimeLocal(pg: string): string {
        if (!pg) return "";
        // Remove timezone (Z, +00:00, +01, etc.) and microseconds
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
    class={cn(
        "flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm text-foreground shadow-xs",
        "focus-visible:outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]",
        "disabled:cursor-not-allowed disabled:bg-muted disabled:text-muted-foreground",
        className,
    )}
/>
