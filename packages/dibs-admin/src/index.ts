// Main entry point for @bearcove/dibs-admin

export { default as DibsAdmin } from "./DibsAdmin.svelte";

// Re-export types that consumers might need
export type {
  SquelClient,
  SchemaInfo,
  TableInfo,
  ColumnInfo,
  Row,
  RowField,
  Value,
  Filter,
  FilterOp,
  Sort,
  SortDir,
  ListRequest,
  ListResponse,
  GetRequest,
  CreateRequest,
  UpdateRequest,
  DeleteRequest,
  DibsError,
  Result,
} from "./types";

// Re-export config types
export type {
  DibsAdminConfig,
  DashboardConfig,
  DashboardTile,
  LatestRecordsTile,
  CountTile,
  QuickLinksTile,
  CustomTile,
  TableConfig,
  ListViewConfig,
  RowExpandConfig,
  DetailConfig,
  FieldGroupConfig,
  RelationConfig,
  SortConfig,
  FilterConfig,
  FieldRenderer,
  TableDefaults,
} from "./types/config";
