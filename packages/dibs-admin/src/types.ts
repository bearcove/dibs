// Re-export types that consumers need
// These match the generated squel-service types

export type FilterOp =
  | { tag: 'Eq' }
  | { tag: 'Ne' }
  | { tag: 'Lt' }
  | { tag: 'Lte' }
  | { tag: 'Gt' }
  | { tag: 'Gte' }
  | { tag: 'Like' }
  | { tag: 'ILike' }
  | { tag: 'IsNull' }
  | { tag: 'IsNotNull' }
  | { tag: 'In' };

export type Value =
  | { tag: 'Null' }
  | { tag: 'Bool'; value: boolean }
  | { tag: 'I16'; value: number }
  | { tag: 'I32'; value: number }
  | { tag: 'I64'; value: bigint }
  | { tag: 'F32'; value: number }
  | { tag: 'F64'; value: number }
  | { tag: 'String'; value: string }
  | { tag: 'Bytes'; value: Uint8Array };

export type SortDir = { tag: 'Asc' } | { tag: 'Desc' };

export interface Filter {
  field: string;
  op: FilterOp;
  value: Value;
  values: Value[];
}

export interface Sort {
  field: string;
  dir: SortDir;
}

export interface ColumnInfo {
  name: string;
  sql_type: string;
  rust_type: string | null;
  nullable: boolean;
  default: string | null;
  primary_key: boolean;
  unique: boolean;
  auto_generated: boolean;
  doc: string | null;
}

export interface ForeignKeyInfo {
  columns: string[];
  references_table: string;
  references_columns: string[];
}

export interface IndexInfo {
  name: string;
  columns: string[];
  unique: boolean;
}

export interface TableInfo {
  name: string;
  columns: ColumnInfo[];
  foreign_keys: ForeignKeyInfo[];
  indices: IndexInfo[];
  source_file: string | null;
  source_line: number | null;
  doc: string | null;
}

export interface SchemaInfo {
  tables: TableInfo[];
}

export interface RowField {
  name: string;
  value: Value;
}

export interface Row {
  fields: RowField[];
}

export interface ListRequest {
  database_url: string;
  table: string;
  filters: Filter[];
  sort: Sort[];
  limit: number | null;
  offset: number | null;
  select: string[];
}

export interface ListResponse {
  rows: Row[];
  total: bigint | null;
}

export interface GetRequest {
  database_url: string;
  table: string;
  pk: Value;
}

export interface CreateRequest {
  database_url: string;
  table: string;
  data: Row;
}

export interface UpdateRequest {
  database_url: string;
  table: string;
  pk: Value;
  data: Row;
}

export interface DeleteRequest {
  database_url: string;
  table: string;
  pk: Value;
}

export type DibsError =
  | { tag: 'ConnectionFailed'; value: string }
  | { tag: 'MigrationFailed'; value: string }
  | { tag: 'InvalidRequest'; value: string }
  | { tag: 'UnknownTable'; value: string }
  | { tag: 'UnknownColumn'; value: string }
  | { tag: 'QueryError'; value: string };

export type Result<T, E> = { ok: true; value: T } | { ok: false; error: E };

/**
 * The client interface that DibsAdmin expects.
 * This matches the generated SquelServiceCaller interface.
 */
export interface SquelClient {
  schema(): Promise<SchemaInfo>;
  list(request: ListRequest): Promise<Result<ListResponse, DibsError>>;
  get(request: GetRequest): Promise<Result<Row | null, DibsError>>;
  create(request: CreateRequest): Promise<Result<Row, DibsError>>;
  update(request: UpdateRequest): Promise<Result<Row, DibsError>>;
  delete(request: DeleteRequest): Promise<Result<bigint, DibsError>>;
}
