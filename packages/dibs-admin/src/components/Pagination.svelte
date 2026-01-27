<script lang="ts">
    import { CaretLeft, CaretRight } from "phosphor-svelte";
    import { Button } from "../lib/ui/index";

    interface Props {
        offset: number;
        limit: number;
        rowCount: number;
        total: bigint | null;
        onPrev: () => void;
        onNext: () => void;
    }

    let { offset, limit, rowCount, total, onPrev, onNext }: Props = $props();

    let hasPrev = $derived(offset > 0);
    let hasNext = $derived(rowCount >= limit);
    let start = $derived(offset + 1);
    let end = $derived(offset + rowCount);
</script>

<div class="pagination">
    <Button variant="ghost" size="sm" onclick={onPrev} disabled={!hasPrev}>
        <CaretLeft size={14} />
        Prev
    </Button>
    <span class="pagination-info">
        {start}–{end}
        {#if total !== null}
            / {total.toString()}
        {/if}
    </span>
    <Button variant="ghost" size="sm" onclick={onNext} disabled={!hasNext}>
        Next
        <CaretRight size={14} />
    </Button>
</div>

<style>
    .pagination {
        display: flex;
        justify-content: center;
        align-items: center;
        gap: 1.5rem;
        margin-top: 2rem;
        padding-top: 1.5rem;
    }

    .pagination-info {
        color: var(--muted-foreground);
        font-size: 0.875rem;
    }
</style>
