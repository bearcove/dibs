import { getContext, setContext } from "svelte";
import type {
    SquelServiceCaller,
    SchemaInfo,
    TableInfo,
    Row,
    Value,
    Filter,
    Sort,
} from "@bearcove/dibs-admin/types";
import type { DibsAdminConfig } from "@bearcove/dibs-admin/types/config";

const ADMIN_CONTEXT_KEY = Symbol("dibs-admin-context");

export interface AdminContext {
    // Core
    client: SquelServiceCaller;
    config: DibsAdminConfig | undefined;

    // Schema (reactive getter)
    get schema(): SchemaInfo | null;

    // Navigation
    navigateToDashboard: () => void;
    navigateToTable: (table: string) => void;
    navigateToRow: (table: string, pk: string) => void;
    navigateToNewRow: (table: string) => void;

    // FK lookup cache
    get fkLookup(): Map<string, Map<string, Row>>;
    setFkLookup: (lookup: Map<string, Map<string, Row>>) => void;

    // Time display mode
    get timeMode(): "relative" | "absolute";
    setTimeMode: (mode: "relative" | "absolute") => void;

    // Breadcrumbs
    get breadcrumbs(): BreadcrumbEntry[];
    setBreadcrumbs: (entries: BreadcrumbEntry[]) => void;
    addBreadcrumb: (entry: BreadcrumbEntry) => void;
}

export interface BreadcrumbEntry {
    table: string;
    label: string;
    pkValue?: Value;
}

export function setAdminContext(ctx: AdminContext): void {
    setContext(ADMIN_CONTEXT_KEY, ctx);
}

export function getAdminContext(): AdminContext {
    const ctx = getContext<AdminContext>(ADMIN_CONTEXT_KEY);
    if (!ctx) {
        throw new Error("AdminContext not found. Make sure this component is inside DibsAdmin.");
    }
    return ctx;
}
