<script lang="ts">
    import * as icons from "@lucide/svelte";
    import type { Component } from "svelte";

    interface Props {
        name: string;
        class?: string;
        size?: number;
    }

    let { name, class: className = "", size = 16 }: Props = $props();

    // Convert kebab-case to PascalCase for icon lookup
    function toPascalCase(str: string): string {
        return str
            .split("-")
            .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
            .join("");
    }

    // Get the icon component by name
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    function getIcon(iconName: string): Component<{ size?: number; class?: string }> | null {
        const pascalName = toPascalCase(iconName);
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const iconsRecord = icons as any;
        const icon = iconsRecord[pascalName];
        if (icon && typeof icon === "function") {
            return icon;
        }
        return null;
    }

    let Icon = $derived(getIcon(name));
</script>

{#if Icon}
    <Icon class={className} {size} />
{/if}
