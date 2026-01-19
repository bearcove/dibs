<script lang="ts">
    import { AlertDialog, Button } from "../lib/components/ui/index.js";
    import { ArrowRight } from "phosphor-svelte";

    interface Change {
        field: string;
        label: string;
        oldValue: string;
        newValue: string;
    }

    interface Props {
        open: boolean;
        changes: Change[];
        saving: boolean;
        onconfirm: () => void;
        oncancel: () => void;
    }

    let { open = $bindable(), changes, saving, onconfirm, oncancel }: Props = $props();

    function formatValue(val: string): string {
        if (val === "" || val === "null") return "—";
        if (val.length > 50) return val.slice(0, 50) + "…";
        return val;
    }
</script>

<AlertDialog.Root bind:open>
    <AlertDialog.Content class="max-w-lg">
        <AlertDialog.Header>
            <AlertDialog.Title>Confirm Changes</AlertDialog.Title>
            <AlertDialog.Description>
                Review the following {changes.length} change{changes.length === 1 ? "" : "s"} before saving.
            </AlertDialog.Description>
        </AlertDialog.Header>

        <div class="my-4 space-y-3 max-h-[300px] overflow-y-auto">
            {#each changes as change}
                <div class="bg-muted/50 rounded-md p-3">
                    <div class="text-sm font-medium text-foreground mb-2">{change.label}</div>
                    <div class="flex items-center gap-2 text-sm">
                        <span class="text-muted-foreground line-through flex-1 min-w-0 truncate" title={change.oldValue}>
                            {formatValue(change.oldValue)}
                        </span>
                        <ArrowRight size={14} class="text-muted-foreground shrink-0" />
                        <span class="text-foreground flex-1 min-w-0 truncate font-medium" title={change.newValue}>
                            {formatValue(change.newValue)}
                        </span>
                    </div>
                </div>
            {/each}
        </div>

        <AlertDialog.Footer>
            <AlertDialog.Cancel disabled={saving} onclick={oncancel}>Cancel</AlertDialog.Cancel>
            <Button variant="default" disabled={saving} onclick={onconfirm}>
                {saving ? "Saving..." : "Save Changes"}
            </Button>
        </AlertDialog.Footer>
    </AlertDialog.Content>
</AlertDialog.Root>
