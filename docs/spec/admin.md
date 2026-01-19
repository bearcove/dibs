# DibsAdmin Specification

DibsAdmin is a generic admin UI component that renders CRUD interfaces for dibs-managed tables.

## Configuration Architecture

Configuration is split between two layers based on where the concern belongs.

r[config.layers]
DibsAdmin MUST support two configuration layers: schema metadata (from Rust) and UI presentation config (from component props).

### Schema Metadata (Rust)

Schema metadata travels with `TableInfo`/`ColumnInfo` from the Rust backend. This is information intrinsic to the data itself.

r[config.schema.label]
The `label` field on `ColumnInfo` MUST indicate this column is the human-readable display name for rows.

r[config.schema.long]
The `long` field on `ColumnInfo` MUST indicate this is long-form text that should use a textarea.

r[config.schema.auto-generated]
The `auto_generated` field on `ColumnInfo` MUST indicate this column should not appear in create forms.

r[config.schema.doc]
The `doc` field on `ColumnInfo` MAY contain documentation for the column.

### UI Presentation Config (Props)

UI presentation config is passed as props to the `<DibsAdmin>` component. This is presentation logic that can vary per deployment.

r[config.props.interface]
DibsAdmin MUST accept an optional `config` prop with the following TypeScript interface:

> r[config.props.admin-config]
> The `AdminConfig` interface MUST have this shape:
>
> ```typescript
> interface AdminConfig {
>   tables?: {
>     [tableName: string]: TableConfig;
>   };
> }
> ```

> r[config.props.table-config]
> The `TableConfig` interface MUST have this shape:
>
> ```typescript
> interface TableConfig {
>   // Table view
>   listColumns?: string[];
>   listRelations?: RelationConfig[];
>
>   // Detail view
>   detailSections?: SectionConfig[];
>   detailRelations?: RelationConfig[];
> }
> ```

> r[config.props.relation-config]
> The `RelationConfig` interface MUST have this shape:
>
> ```typescript
> interface RelationConfig {
>   table: string;
>   foreignKey: string;
>   display?: 'inline' | 'tab';
>   limit?: number;
> }
> ```

> r[config.props.section-config]
> The `SectionConfig` interface MUST have this shape:
>
> ```typescript
> interface SectionConfig {
>   title: string;
>   columns: string[];
> }
> ```

## Table View

The table view displays a paginated list of rows from a table.

### Default Column Selection

r[table.columns.default]
When no `listColumns` config is provided, DibsAdmin MUST show all columns except those marked as `long: true` or containing binary data.

r[table.columns.order]
Columns MUST be displayed in the order defined in the schema.

r[table.columns.configured]
When `listColumns` is provided, only those columns MUST be shown, in the order specified.

### Foreign Key Display

r[table.fk.display-value]
Foreign key columns MUST display the referenced row's label column value instead of the raw ID.

r[table.fk.fallback]
If the referenced table has no `label` column, the display MUST fall back to the primary key value.

r[table.fk.batch-lookup]
FK display values MUST be loaded via batch lookup to avoid N+1 queries.

r[table.fk.clickable]
FK values MUST be clickable, navigating to the referenced row's detail view.

### Sorting

r[table.sort.default]
The default sort MUST be by primary key descending (newest first).

r[table.sort.clickable]
Column headers MUST be clickable to change sort order.

r[table.sort.indicator]
The currently sorted column MUST display a visual indicator of sort direction.

## Detail View

The detail view displays a single row with all its data and related records.

### Field Display

r[detail.fields.all]
The detail view MUST show all columns by default.

r[detail.fields.sections]
When `detailSections` is configured, fields MUST be grouped into the specified sections.

r[detail.fields.fk-links]
FK columns MUST be rendered as links to the referenced record.

### Related Records

r[detail.relations.none-by-default]
Related records MUST NOT be shown unless explicitly configured via `detailRelations`.

r[detail.relations.direction]
The system MUST detect relation direction automatically:
- If `foreignKey` is on the current table, it's a belongs-to (single record)
- If `foreignKey` is on the related table, it's a has-many (multiple records)

r[detail.relations.display-inline]
Relations with `display: 'inline'` MUST be rendered as an embedded list below the main fields.

r[detail.relations.display-tab]
Relations with `display: 'tab'` MUST be rendered in a separate tab.

r[detail.relations.limit]
Relations MUST respect the `limit` config, defaulting to 10 records.

## Row Editor

The row editor handles create and update operations.

r[editor.create.hide-auto]
Auto-generated columns MUST be hidden in create forms.

r[editor.create.defaults]
Columns with defaults MUST show the default value as a placeholder.

r[editor.update.show-all]
Update forms MUST show all editable columns.

r[editor.fk.select]
FK columns MUST render as a searchable select component.

r[editor.long.textarea]
Columns marked `long: true` MUST render as a textarea.

r[editor.enum.dropdown]
Columns with `enum_variants` MUST render as a dropdown select.

## Open Questions

These are unresolved design questions for future consideration:

- Should `listColumns` support negative selection? e.g., `['*', '-password_hash']`
- Should there be a Rust-side hint for "hidden in admin" vs purely frontend config?
- Computed/virtual columns? e.g., showing `full_name` derived from `first_name` + `last_name`
- Column aliases? Showing "Email Address" instead of "email" in headers
- Custom cell renderers? For things like status badges, image previews, etc.
