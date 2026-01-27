<script lang="ts">
    import { AlertDialog, Button } from "@bearcove/dibs-admin/lib/ui";
    import ArrowRightIcon from "phosphor-svelte/lib/ArrowRightIcon";

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
    <AlertDialog.Content class="confirm-dialog">
        <AlertDialog.Header>
            <AlertDialog.Title>Confirm Changes</AlertDialog.Title>
            <AlertDialog.Description>
                Review the following {changes.length} change{changes.length === 1 ? "" : "s"} before saving.
            </AlertDialog.Description>
        </AlertDialog.Header>

        <div class="changes-list">
            {#each changes as change}
                <div class="change-item">
                    <div class="change-label">{change.label}</div>
                    <div class="change-values">
                        <span class="old-value" title={change.oldValue}>
                            {formatValue(change.oldValue)}
                        </span>
                        <ArrowRightIcon size={14} class="arrow-icon" />
                        <span class="new-value" title={change.newValue}>
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

<style>
    :global(.confirm-dialog) {
        max-width: 32rem;
    }

    .changes-list {
        margin: 1rem 0;
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
        max-height: 300px;
        overflow-y: auto;
    }

    .change-item {
        background-color: oklch(from var(--muted) l c h / 0.5);
        border-radius: var(--radius-md, 0.375rem);
        padding: 0.75rem;
    }

    .change-label {
        font-size: 0.875rem;
        font-weight: 500;
        color: var(--foreground);
        margin-bottom: 0.5rem;
    }

    .change-values {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        font-size: 0.875rem;
    }

    .old-value {
        color: var(--muted-foreground);
        text-decoration: line-through;
        flex: 1;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    :global(.arrow-icon) {
        color: var(--muted-foreground);
        flex-shrink: 0;
    }

    .new-value {
        color: var(--foreground);
        flex: 1;
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-weight: 500;
    }
</style>
