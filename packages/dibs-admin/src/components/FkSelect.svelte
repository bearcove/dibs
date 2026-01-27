<script lang="ts">
    import type { TableInfo, SquelClient, ListRequest } from "../types.js";
    import { getDisplayColumn, getPkValue, formatValueForDisplay } from "../lib/fk-utils.js";
    import { Select } from "../lib/components/ui/index.js";

    interface Props {
        value: string;
        fkTable: TableInfo;
        client: SquelClient;

        disabled?: boolean;
        onchange: (value: string) => void;
    }

    let { value = $bindable(), fkTable, client, disabled = false, onchange }: Props = $props();

    let options = $state<{ value: string; label: string }[]>([]);
    let loading = $state(true);
    let error = $state<string | null>(null);

    // Get the display column and PK column for the FK table
    let displayCol = $derived(getDisplayColumn(fkTable));
    let pkCol = $derived(fkTable.columns.find((c) => c.primary_key));

    // Load options when component mounts or fkTable changes
    $effect(() => {
        // Capture dependencies
        const tableName = fkTable.name;
        const pk = pkCol;
        const display = displayCol;

        if (!pk) {
            error = "No primary key found";
            loading = false;
            return;
        }

        loading = true;
        error = null;

        const request: ListRequest = {
            table: tableName,
            filters: [],
            sort: display ? [{ field: display.name, dir: { tag: "Asc" } }] : [],
            limit: 100,
            offset: null,
            select: [],
        };

        client
            .list(request)
            .then((result) => {
                if (result.ok) {
                    options = result.value.rows.map((row) => {
                        const pkValue = getPkValue(fkTable, row);
                        const pkStr = pkValue ? formatValueForDisplay(pkValue) : "";

                        // Get display value
                        let label = pkStr;
                        if (display) {
                            const displayField = row.fields.find((f) => f.name === display.name);
                            if (displayField && displayField.value.tag !== "Null") {
                                const displayValue = formatValueForDisplay(displayField.value);
                                label = `${displayValue} (${pkStr})`;
                            }
                        }

                        return { value: pkStr, label };
                    });
                } else {
                    error = result.error.value;
                }
                loading = false;
            })
            .catch((e) => {
                error = e instanceof Error ? e.message : String(e);
                loading = false;
            });
    });
</script>

{#if loading}
    <div class="h-9 flex items-center px-3 bg-muted text-muted-foreground text-sm">
        Loading options...
    </div>
{:else if error}
    <div class="text-destructive text-sm">{error}</div>
{:else}
    <Select.Root
        type="single"
        bind:value
        {disabled}
        onValueChange={(v: string) => {
            value = v;
            onchange(v);
        }}
    >
        <Select.Trigger class="w-full">
            {#if value}
                {options.find((o) => o.value === value)?.label ?? value}
            {:else}
                -- Select {fkTable.name} --
            {/if}
        </Select.Trigger>
        <Select.Content>
            <Select.Item value="">-- Select {fkTable.name} --</Select.Item>
            {#each options as opt}
                <Select.Item value={opt.value}>{opt.label}</Select.Item>
            {/each}
        </Select.Content>
    </Select.Root>
{/if}
