<script lang="ts">
    import { cn } from "$lib/utils.js";
    import type { HTMLInputAttributes } from "svelte/elements";

    interface Props extends Omit<HTMLInputAttributes, "type"> {
        class?: string;
    }

    let { class: className, ...rest }: Props = $props();
</script>

<input
    type="number"
    class={cn(
        "flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm text-foreground placeholder:text-muted-foreground shadow-xs",
        "focus-visible:outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px]",
        "disabled:cursor-not-allowed disabled:bg-muted disabled:text-muted-foreground",
        "[appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none",
        className,
    )}
    {...rest}
/>
