<script lang="ts">
    import { cn } from "../../utils.js";
    import type { HTMLSelectAttributes } from "svelte/elements";

    interface Props extends HTMLSelectAttributes {
        class?: string;
        value?: string;
    }

    let { class: className, children, value = $bindable(""), ...rest }: Props = $props();
</script>

<select
    bind:value
    class={cn(
        "flex h-9 w-full bg-input px-3 py-1 text-sm text-foreground focus-visible:outline-none focus-visible:bg-accent disabled:cursor-not-allowed disabled:opacity-40",
        className
    )}
    {...rest}
>
    {@render children?.()}
</select>
