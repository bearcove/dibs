// Re-export all types from the squel-service package
export type {
  FilterOp,
  Value,
  SortDir,
  Filter,
  Sort,
  ColumnInfo,
  ForeignKeyInfo,
  IndexInfo,
  IndexColumnInfo,
  TableInfo,
  SchemaInfo,
  RowField,
  Row,
  ListRequest,
  ListResponse,
  GetRequest,
  CreateRequest,
  UpdateRequest,
  DeleteRequest,
  DibsError,
  GetResponse,
  CreateResponse,
  UpdateResponse,
  DeleteResponse,
  SquelServiceCaller,
} from "@bearcove/squel-service";

// Re-export the client class
export { SquelServiceClient } from "@bearcove/squel-service";

// Alias for backwards compatibility
export type { SquelServiceCaller as SquelClient } from "@bearcove/squel-service";

// Result type for error handling
export type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };
