<script lang="ts">
    import { cn } from "../../utils.js";
    import type { HTMLSelectAttributes } from "svelte/elements";

    interface Props extends Omit<HTMLSelectAttributes, "value"> {
        class?: string;
        value?: string;
    }

    let { class: className, children, value = $bindable(), ...rest }: Props = $props();
</script>

<select
    class={cn(
        "flex h-9 w-full bg-neutral-900 px-3 py-1 text-sm text-white focus-visible:outline-none focus-visible:bg-neutral-800 disabled:cursor-not-allowed disabled:opacity-40",
        className,
    )}
    bind:value
    {...rest}
>
    {@render children?.()}
</select>
