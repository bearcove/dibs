//! Deterministic PostgreSQL 18 SQL rendering from completed typed query artifacts.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use dibs_pg_catalog::{
    CallableId, CollationId, ColumnId, ConstraintId, OperatorId, TableId, TypeId,
};
use dibs_query_ir::{
    CompiledQuery, CompiledQueryError, ConflictTarget, CteId, CteMaterialization, FrameBound,
    HirLiteral, HirLockClause, JoinKind, LockStrength, LockWaitPolicy, NullsOrder, OrderedBind,
    ParameterId, RelationAlias, RelationId, SelectDistinct, SetOperationKind, SortDirection,
    TypedArgument, TypedAssignment, TypedCall, TypedCoercion, TypedConflictAction,
    TypedConflictClause, TypedCte, TypedDelete, TypedExpression, TypedExpressionKind, TypedInsert,
    TypedInsertSource, TypedLimit, TypedOrderBy, TypedProjection, TypedRelation, TypedRelationKind,
    TypedSelect, TypedStatement, TypedStatementKind, TypedUpdate, TypedValues, WindowExclusion,
    WindowFrame, WindowFrameMode, WindowReference, WindowSpec,
};

/// Complete deterministic SQL plus the declaration-ordered bind map it uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedSql {
    /// Static PostgreSQL 18 statement text.
    pub sql: String,
    /// One entry per distinct declared parameter referenced by the statement.
    pub ordered_binds: Vec<OrderedBind>,
}

/// An immutable artifact contains a fact that cannot be rendered soundly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlRenderError {
    /// The immutable artifact fails its own cross-surface invariants.
    InvalidCompiledQuery(CompiledQueryError),
    /// The artifact targets a PostgreSQL major other than 18.
    UnsupportedPostgresMajor(u16),
    /// Two ordered declarations reuse one revision-local parameter identity.
    DuplicateDeclaredParameter(ParameterId),
    /// Typed topology references a parameter absent from ordered declarations.
    UnknownParameter(ParameterId),
    /// Typed topology references a relation binding that is not visible here.
    UnknownRelation(RelationId),
    /// Typed topology references a CTE not visible here.
    UnknownCte(CteId),
    /// Typed topology references a CTE output field absent from its declaration.
    UnknownCteField {
        /// Referenced CTE.
        cte_id: CteId,
        /// Revision-local field value.
        field_id: u32,
    },
    /// A stable catalog identity lacks its canonical rendering fact.
    MissingCatalogName(String),
    /// A catalog operator render fact is not a valid SQL operator token sequence.
    InvalidOperator(String),
    /// A typed semantic literal has a non-canonical or unsafe spelling.
    InvalidLiteral,
    /// A typed typmod retained an invalid canonical spelling.
    InvalidTypmod,
    /// The completed typed artifact violates a renderer-required topology invariant.
    InvalidArtifact(&'static str),
}

impl std::fmt::Display for SqlRenderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "cannot render compiled SQL: {self:?}")
    }
}

impl std::error::Error for SqlRenderError {}

/// Renders a completed compiled-query artifact as static PostgreSQL 18 SQL.
///
/// Catalog names, authored aliases/labels, CTE names, and parameter declaration
/// order are consumed directly from the artifact. Stable identities are never
/// parsed, catalog state is never consulted, and runtime SQL construction is not
/// involved.
pub fn render_compiled_sql(query: &CompiledQuery) -> Result<RenderedSql, SqlRenderError> {
    query
        .validate()
        .map_err(SqlRenderError::InvalidCompiledQuery)?;
    if query.compiler_versions.supported_postgres_major != 18 {
        return Err(SqlRenderError::UnsupportedPostgresMajor(
            query.compiler_versions.supported_postgres_major,
        ));
    }

    let mut parameter_positions = BTreeMap::new();
    let mut next_position = 1u32;
    for parameter in &query.ordered_parameters {
        if parameter_positions.contains_key(&parameter.id) {
            return Err(SqlRenderError::DuplicateDeclaredParameter(parameter.id));
        }
        parameter_positions.insert(parameter.id, next_position);
        next_position = next_position
            .checked_add(1)
            .ok_or(SqlRenderError::InvalidArtifact("too many parameters"))?;
    }

    let mut renderer = Renderer {
        query,
        parameter_positions,
        used_parameters: BTreeSet::new(),
        relation_scopes: Vec::new(),
        cte_scopes: Vec::new(),
        target_scopes: Vec::new(),
        conflict_target_depth: 0,
    };
    let mut sql = String::new();
    renderer.render_statement(&mut sql, &query.typed_statement)?;
    let ordered_binds = query
        .ordered_parameters
        .iter()
        .filter(|parameter| renderer.used_parameters.contains(&parameter.id))
        .map(|parameter| {
            Ok(OrderedBind {
                position: renderer
                    .parameter_positions
                    .get(&parameter.id)
                    .copied()
                    .ok_or(SqlRenderError::UnknownParameter(parameter.id))?,
                parameter_id: parameter.id,
            })
        })
        .collect::<Result<Vec<_>, SqlRenderError>>()?;

    Ok(RenderedSql { sql, ordered_binds })
}

struct Renderer<'a> {
    query: &'a CompiledQuery,
    parameter_positions: BTreeMap<ParameterId, u32>,
    used_parameters: BTreeSet<ParameterId>,
    relation_scopes: Vec<BTreeMap<RelationId, String>>,
    cte_scopes: Vec<BTreeMap<CteId, CteRender<'a>>>,
    target_scopes: Vec<BTreeSet<RelationId>>,
    conflict_target_depth: usize,
}

#[derive(Clone, Copy)]
struct CteRender<'a> {
    name: &'a str,
    fields: &'a [dibs_query_ir::FieldId],
    output_names: &'a [String],
}

impl<'a> Renderer<'a> {
    fn render_statement(
        &mut self,
        sql: &mut String,
        statement: &'a TypedStatement,
    ) -> Result<(), SqlRenderError> {
        match &statement.kind {
            TypedStatementKind::Select(select) => self.render_select(sql, select),
            TypedStatementKind::Insert(insert) => self.render_insert(sql, insert),
            TypedStatementKind::Update(update) => self.render_update(sql, update),
            TypedStatementKind::Delete(delete) => self.render_delete(sql, delete),
        }
    }

    fn render_select(
        &mut self,
        sql: &mut String,
        select: &'a TypedSelect,
    ) -> Result<(), SqlRenderError> {
        self.push_ctes(&select.ctes);
        self.push_relations(&select.from)?;
        let result = (|| {
            self.render_with(sql, select.recursive, &select.ctes)?;
            sql.push_str("SELECT");
            match &select.distinct {
                SelectDistinct::AllRows => {}
                SelectDistinct::Distinct => sql.push_str(" DISTINCT"),
                SelectDistinct::On(expressions) => {
                    if expressions.is_empty() {
                        return Err(SqlRenderError::InvalidArtifact("empty DISTINCT ON"));
                    }
                    sql.push_str(" DISTINCT ON (");
                    self.render_expression_list(sql, expressions)?;
                    sql.push(')');
                }
            }
            if select.projections.is_empty() {
                return Err(SqlRenderError::InvalidArtifact("empty SELECT projection"));
            }
            sql.push(' ');
            self.render_projections(sql, &select.projections)?;

            if !select.from.is_empty() {
                sql.push_str(" FROM ");
                self.render_relation_list(sql, &select.from)?;
            }
            if let Some(predicate) = &select.predicate {
                sql.push_str(" WHERE ");
                self.render_expression(sql, predicate)?;
            }
            if !select.group_by.is_empty() {
                sql.push_str(" GROUP BY ");
                self.render_expression_list(sql, &select.group_by)?;
            }
            if let Some(having) = &select.having {
                sql.push_str(" HAVING ");
                self.render_expression(sql, having)?;
            }
            if !select.windows.is_empty() {
                sql.push_str(" WINDOW ");
                for (index, window) in select.windows.iter().enumerate() {
                    comma(sql, index);
                    quote_identifier(sql, &window.name);
                    sql.push_str(" AS (");
                    self.render_window_spec(sql, &window.specification)?;
                    sql.push(')');
                }
            }
            self.render_order_by_clause(sql, &select.order_by)?;
            if let Some(limit) = &select.limit {
                sql.push_str(" LIMIT ");
                self.render_limit(sql, limit)?;
            }
            if let Some(offset) = &select.offset {
                sql.push_str(" OFFSET ");
                self.render_limit(sql, offset)?;
            }
            for lock in &select.locks {
                self.render_lock(sql, lock)?;
            }
            Ok(())
        })();
        self.relation_scopes.pop();
        self.cte_scopes.pop();
        result
    }

    fn render_insert(
        &mut self,
        sql: &mut String,
        insert: &'a TypedInsert,
    ) -> Result<(), SqlRenderError> {
        self.push_ctes(&insert.ctes);
        let result = (|| {
            self.render_with(sql, false, &insert.ctes)?;
            sql.push_str("INSERT INTO ");
            self.render_table_name(sql, &insert.target)?;
            if !insert.columns.is_empty() {
                sql.push_str(" (");
                for (index, column) in insert.columns.iter().enumerate() {
                    comma(sql, index);
                    self.render_column_name(sql, column)?;
                }
                sql.push(')');
            }
            sql.push(' ');
            match &insert.source {
                TypedInsertSource::Values(values) => self.render_values(sql, values)?,
                TypedInsertSource::Select(statement) => self.render_statement(sql, statement)?,
                TypedInsertSource::DefaultValues => sql.push_str("DEFAULT VALUES"),
            }
            self.push_target_relation(insert.target.clone(), Some(RelationId::new(1)));
            let target_result = (|| {
                if let Some(conflict) = &insert.conflict {
                    self.render_conflict(sql, conflict)?;
                }
                self.render_returning(sql, &insert.returning)
            })();
            self.relation_scopes.pop();
            self.target_scopes.pop();
            target_result
        })();
        self.cte_scopes.pop();
        result
    }

    fn render_update(
        &mut self,
        sql: &mut String,
        update: &'a TypedUpdate,
    ) -> Result<(), SqlRenderError> {
        self.push_ctes(&update.ctes);
        let result = (|| {
            self.render_with(sql, false, &update.ctes)?;
            sql.push_str("UPDATE ");
            self.render_table_name(sql, &update.target)?;
            self.push_target_relation(update.target.clone(), Some(update.target_binding));
            let target_result = (|| {
                for relation in &update.from {
                    self.register_relation_tree(relation)?;
                }
                sql.push_str(" SET ");
                self.render_assignments(sql, &update.assignments)?;
                if !update.from.is_empty() {
                    sql.push_str(" FROM ");
                    self.render_relation_list(sql, &update.from)?;
                }
                if let Some(predicate) = &update.predicate {
                    sql.push_str(" WHERE ");
                    self.render_expression(sql, predicate)?;
                }
                self.render_returning(sql, &update.returning)
            })();
            self.target_scopes.pop();
            self.relation_scopes.pop();
            target_result
        })();
        self.cte_scopes.pop();
        result
    }

    fn render_delete(
        &mut self,
        sql: &mut String,
        delete: &'a TypedDelete,
    ) -> Result<(), SqlRenderError> {
        self.push_ctes(&delete.ctes);
        let result = (|| {
            self.render_with(sql, false, &delete.ctes)?;
            sql.push_str("DELETE FROM ");
            self.render_table_name(sql, &delete.target)?;
            self.push_target_relation(delete.target.clone(), Some(delete.target_binding));
            let target_result = (|| {
                for relation in &delete.using_relations {
                    self.register_relation_tree(relation)?;
                }
                if !delete.using_relations.is_empty() {
                    sql.push_str(" USING ");
                    self.render_relation_list(sql, &delete.using_relations)?;
                }
                if let Some(predicate) = &delete.predicate {
                    sql.push_str(" WHERE ");
                    self.render_expression(sql, predicate)?;
                }
                self.render_returning(sql, &delete.returning)
            })();
            self.target_scopes.pop();
            self.relation_scopes.pop();
            target_result
        })();
        self.cte_scopes.pop();
        result
    }

    fn render_with(
        &mut self,
        sql: &mut String,
        recursive: bool,
        ctes: &'a [TypedCte],
    ) -> Result<(), SqlRenderError> {
        if ctes.is_empty() {
            return Ok(());
        }
        sql.push_str("WITH ");
        if recursive {
            sql.push_str("RECURSIVE ");
        }
        for (index, cte) in ctes.iter().enumerate() {
            comma(sql, index);
            quote_identifier(sql, cte.name());
            if !cte.output_names().is_empty() {
                sql.push_str(" (");
                for (field_index, name) in cte.output_names().iter().enumerate() {
                    comma(sql, field_index);
                    quote_identifier(sql, name);
                }
                sql.push(')');
            }
            sql.push_str(" AS");
            match cte.materialization {
                CteMaterialization::Default => {}
                CteMaterialization::Materialized => sql.push_str(" MATERIALIZED"),
                CteMaterialization::NotMaterialized => sql.push_str(" NOT MATERIALIZED"),
            }
            sql.push_str(" (");
            self.render_statement(sql, &cte.statement)?;
            sql.push(')');
        }
        sql.push(' ');
        Ok(())
    }

    fn render_projections(
        &mut self,
        sql: &mut String,
        projections: &'a [TypedProjection],
    ) -> Result<(), SqlRenderError> {
        for (index, projection) in projections.iter().enumerate() {
            comma(sql, index);
            self.render_expression(sql, &projection.expression)?;
            sql.push_str(" AS ");
            quote_identifier(sql, &projection.sql_label);
        }
        Ok(())
    }

    fn render_returning(
        &mut self,
        sql: &mut String,
        projections: &'a [TypedProjection],
    ) -> Result<(), SqlRenderError> {
        if !projections.is_empty() {
            sql.push_str(" RETURNING ");
            self.render_projections(sql, projections)?;
        }
        Ok(())
    }

    fn render_relation_list(
        &mut self,
        sql: &mut String,
        relations: &'a [TypedRelation],
    ) -> Result<(), SqlRenderError> {
        for (index, relation) in relations.iter().enumerate() {
            comma(sql, index);
            self.render_relation(sql, relation, false)?;
        }
        Ok(())
    }

    fn render_relation(
        &mut self,
        sql: &mut String,
        relation: &'a TypedRelation,
        force_parentheses: bool,
    ) -> Result<(), SqlRenderError> {
        let is_join = matches!(relation.kind, TypedRelationKind::Join { .. });
        if force_parentheses && is_join {
            sql.push('(');
        }
        match &relation.kind {
            TypedRelationKind::Table { table_id } => self.render_table_name(sql, table_id)?,
            TypedRelationKind::Cte { cte_id } => {
                let cte = self.cte(cte_id)?;
                quote_identifier(sql, cte.name);
            }
            TypedRelationKind::Subquery(statement) => {
                sql.push('(');
                self.render_statement(sql, statement)?;
                sql.push(')');
            }
            TypedRelationKind::Function {
                callable_id,
                arguments,
            } => {
                self.render_callable_name(sql, callable_id)?;
                sql.push('(');
                self.render_expression_list(sql, arguments)?;
                sql.push(')');
            }
            TypedRelationKind::Join {
                kind,
                left,
                right,
                predicate,
                lateral,
            } => {
                self.render_relation(sql, left, true)?;
                sql.push(' ');
                sql.push_str(match kind {
                    JoinKind::Inner => "INNER JOIN",
                    JoinKind::Left => "LEFT JOIN",
                    JoinKind::Right => "RIGHT JOIN",
                    JoinKind::Full => "FULL JOIN",
                    JoinKind::Cross => "CROSS JOIN",
                });
                sql.push(' ');
                if *lateral {
                    sql.push_str("LATERAL ");
                }
                self.render_relation(sql, right, true)?;
                match (kind, predicate) {
                    (JoinKind::Cross, None) => {}
                    (JoinKind::Cross, Some(_)) => {
                        return Err(SqlRenderError::InvalidArtifact(
                            "CROSS JOIN cannot carry an ON predicate",
                        ));
                    }
                    (_, Some(predicate)) => {
                        sql.push_str(" ON ");
                        self.render_expression(sql, predicate)?;
                    }
                    (_, None) => {
                        return Err(SqlRenderError::InvalidArtifact(
                            "non-cross JOIN requires an ON predicate",
                        ));
                    }
                }
            }
            TypedRelationKind::Values { rows } => {
                sql.push('(');
                self.render_values(sql, rows)?;
                sql.push(')');
            }
            TypedRelationKind::SetOperation {
                kind,
                all,
                left,
                right,
            } => {
                sql.push('(');
                sql.push('(');
                self.render_statement(sql, left)?;
                sql.push(')');
                sql.push(' ');
                sql.push_str(match kind {
                    SetOperationKind::Union => "UNION",
                    SetOperationKind::Intersect => "INTERSECT",
                    SetOperationKind::Except => "EXCEPT",
                });
                if *all {
                    sql.push_str(" ALL");
                }
                sql.push(' ');
                sql.push('(');
                self.render_statement(sql, right)?;
                sql.push(')');
                sql.push(')');
            }
        }
        if let Some(alias) = &relation.alias {
            self.render_relation_alias(sql, alias);
        }
        if force_parentheses && is_join {
            sql.push(')');
        }
        Ok(())
    }

    fn render_relation_alias(&self, sql: &mut String, alias: &RelationAlias) {
        sql.push_str(" AS ");
        quote_identifier(sql, &alias.name);
        if !alias.column_names.is_empty() {
            sql.push_str(" (");
            for (index, name) in alias.column_names.iter().enumerate() {
                comma(sql, index);
                quote_identifier(sql, name);
            }
            sql.push(')');
        }
    }

    fn render_values(
        &mut self,
        sql: &mut String,
        values: &'a TypedValues,
    ) -> Result<(), SqlRenderError> {
        sql.push_str("VALUES ");
        for (row_index, row) in values.rows().iter().enumerate() {
            comma(sql, row_index);
            sql.push('(');
            self.render_expression_list(sql, row)?;
            sql.push(')');
        }
        Ok(())
    }

    fn render_expression_list(
        &mut self,
        sql: &mut String,
        expressions: &'a [TypedExpression],
    ) -> Result<(), SqlRenderError> {
        for (index, expression) in expressions.iter().enumerate() {
            comma(sql, index);
            self.render_expression(sql, expression)?;
        }
        Ok(())
    }

    fn render_expression(
        &mut self,
        sql: &mut String,
        expression: &'a TypedExpression,
    ) -> Result<(), SqlRenderError> {
        match &expression.kind {
            TypedExpressionKind::Literal(literal) => self.render_literal(sql, literal),
            TypedExpressionKind::Parameter(parameter_id) => {
                self.render_parameter(sql, *parameter_id)
            }
            TypedExpressionKind::Column { binding, column_id } => {
                if self.is_target_binding(*binding) {
                    if self.in_conflict_target() {
                        self.render_column_name(sql, column_id)?;
                        return Ok(());
                    }
                    let table = self.target_table()?;
                    self.render_table_name(sql, &table)?;
                } else {
                    let qualifier = self.relation_qualifier(*binding)?;
                    quote_identifier(sql, &qualifier);
                }
                sql.push('.');
                self.render_column_name(sql, column_id)
            }
            TypedExpressionKind::Call(call) => self.render_call(sql, call),
            TypedExpressionKind::Operator {
                operator_id,
                operands,
            } => self.render_operator(sql, operator_id, operands),
            TypedExpressionKind::Cast {
                expression,
                coercion,
                ..
            } => {
                self.render_expression(sql, expression)?;
                self.render_coercion(sql, coercion)
            }
            TypedExpressionKind::Collate {
                collation_id,
                expression,
            } => {
                self.render_expression(sql, expression)?;
                sql.push_str(" COLLATE ");
                self.render_collation_name(sql, collation_id)
            }
            TypedExpressionKind::Case {
                operand,
                branches,
                else_expression,
                ..
            } => {
                sql.push_str("CASE");
                if let Some(operand) = operand {
                    sql.push(' ');
                    self.render_expression(sql, operand)?;
                }
                for branch in branches {
                    sql.push_str(" WHEN ");
                    self.render_expression(sql, &branch.when)?;
                    sql.push_str(" THEN ");
                    self.render_expression(sql, &branch.then)?;
                }
                if let Some(else_expression) = else_expression {
                    sql.push_str(" ELSE ");
                    self.render_expression(sql, else_expression)?;
                }
                sql.push_str(" END");
                Ok(())
            }
            TypedExpressionKind::ScalarSubquery(statement) => {
                sql.push('(');
                self.render_statement(sql, statement)?;
                sql.push(')');
                Ok(())
            }
            TypedExpressionKind::Row(values) => {
                sql.push_str("ROW(");
                self.render_expression_list(sql, values)?;
                sql.push(')');
                Ok(())
            }
            TypedExpressionKind::Array { elements, .. } => {
                sql.push_str("ARRAY[");
                self.render_expression_list(sql, elements)?;
                sql.push(']');
                Ok(())
            }
            TypedExpressionKind::CteColumn { cte_id, field_id } => {
                let cte = self.cte(cte_id)?;
                let Some(index) = cte
                    .fields
                    .iter()
                    .position(|candidate| candidate == field_id)
                else {
                    return Err(SqlRenderError::UnknownCteField {
                        cte_id: *cte_id,
                        field_id: field_id.get(),
                    });
                };
                let Some(output_name) = cte.output_names.get(index) else {
                    return Err(SqlRenderError::InvalidArtifact("CTE output name arity"));
                };
                quote_identifier(sql, cte.name);
                sql.push('.');
                quote_identifier(sql, output_name);
                Ok(())
            }
        }
    }

    fn render_literal(&self, sql: &mut String, literal: &HirLiteral) -> Result<(), SqlRenderError> {
        match literal {
            HirLiteral::Null => sql.push_str("NULL"),
            HirLiteral::Boolean(true) => sql.push_str("TRUE"),
            HirLiteral::Boolean(false) => sql.push_str("FALSE"),
            HirLiteral::Integer(value) => {
                if !valid_integer(value) {
                    return Err(SqlRenderError::InvalidLiteral);
                }
                sql.push_str(value);
            }
            HirLiteral::Numeric(value) => {
                if !valid_numeric(value) {
                    return Err(SqlRenderError::InvalidLiteral);
                }
                sql.push_str(value);
            }
            HirLiteral::String(value) => quote_string(sql, value),
            HirLiteral::Bytes(value) => {
                sql.push_str("'\\x");
                for byte in value {
                    write!(sql, "{byte:02x}").unwrap();
                }
                sql.push_str("'::bytea");
            }
        }
        Ok(())
    }

    fn render_parameter(
        &mut self,
        sql: &mut String,
        parameter_id: ParameterId,
    ) -> Result<(), SqlRenderError> {
        let position = self
            .parameter_positions
            .get(&parameter_id)
            .copied()
            .ok_or(SqlRenderError::UnknownParameter(parameter_id))?;
        self.used_parameters.insert(parameter_id);
        write!(sql, "${position}").unwrap();
        Ok(())
    }

    fn render_call(&mut self, sql: &mut String, call: &'a TypedCall) -> Result<(), SqlRenderError> {
        self.render_callable_name(sql, &call.callable_id)?;
        sql.push('(');
        if call.star {
            sql.push('*');
        } else {
            if call.distinct {
                sql.push_str("DISTINCT ");
            }
            self.render_arguments(sql, &call.arguments)?;
            if !call.order_by.is_empty() {
                if !call.arguments.is_empty() {
                    sql.push(' ');
                }
                sql.push_str("ORDER BY ");
                self.render_ordering(sql, &call.order_by)?;
            }
        }
        sql.push(')');
        if !call.within_group.is_empty() {
            sql.push_str(" WITHIN GROUP (ORDER BY ");
            self.render_ordering(sql, &call.within_group)?;
            sql.push(')');
        }
        if let Some(filter) = &call.filter {
            sql.push_str(" FILTER (WHERE ");
            self.render_expression(sql, filter)?;
            sql.push(')');
        }
        if let Some(over) = &call.over {
            sql.push_str(" OVER ");
            match over {
                WindowReference::Named(name) => quote_identifier(sql, name),
                WindowReference::Inline(specification) => {
                    sql.push('(');
                    self.render_window_spec(sql, specification)?;
                    sql.push(')');
                }
            }
        }
        Ok(())
    }

    fn render_arguments(
        &mut self,
        sql: &mut String,
        arguments: &'a [TypedArgument],
    ) -> Result<(), SqlRenderError> {
        for (index, argument) in arguments.iter().enumerate() {
            comma(sql, index);
            self.render_expression(sql, &argument.expression)?;
            if let Some(coercion) = &argument.coercion {
                self.render_coercion(sql, coercion)?;
            }
        }
        Ok(())
    }

    fn render_operator(
        &mut self,
        sql: &mut String,
        operator_id: &OperatorId,
        operands: &'a [TypedArgument],
    ) -> Result<(), SqlRenderError> {
        let components = self
            .query
            .catalog_render_names
            .operator(operator_id)
            .ok_or_else(|| SqlRenderError::MissingCatalogName(operator_id.as_str().to_string()))?;
        let (schema, token) = split_operator(components)?;
        match operands {
            [operand] if prefix_operator(token) => {
                self.render_operator_token(sql, schema, token)?;
                sql.push(' ');
                if matches!(
                    operand.expression.kind,
                    TypedExpressionKind::Operator { .. }
                ) {
                    self.render_argument(sql, operand)?;
                } else {
                    sql.push('(');
                    self.render_argument(sql, operand)?;
                    sql.push(')');
                }
            }
            [operand] => {
                sql.push('(');
                self.render_argument(sql, operand)?;
                sql.push(' ');
                self.render_operator_token(sql, schema, token)?;
                sql.push(')');
            }
            [left, right] => {
                self.render_argument(sql, left)?;
                sql.push(' ');
                self.render_operator_token(sql, schema, token)?;
                sql.push(' ');
                self.render_argument(sql, right)?;
            }
            _ => return Err(SqlRenderError::InvalidArtifact("operator arity")),
        }
        Ok(())
    }

    fn render_argument(
        &mut self,
        sql: &mut String,
        argument: &'a TypedArgument,
    ) -> Result<(), SqlRenderError> {
        self.render_expression(sql, &argument.expression)?;
        if let Some(coercion) = &argument.coercion {
            self.render_coercion(sql, coercion)?;
        }
        Ok(())
    }

    fn render_operator_token(
        &self,
        sql: &mut String,
        schema: Option<&str>,
        token: &str,
    ) -> Result<(), SqlRenderError> {
        if !valid_operator_token(token) {
            return Err(SqlRenderError::InvalidOperator(token.to_string()));
        }
        if let Some(schema) = schema
            && symbolic_operator(token)
            && schema != "pg_catalog"
        {
            sql.push_str("OPERATOR(");
            quote_identifier(sql, schema);
            sql.push('.');
            sql.push_str(token);
            sql.push(')');
        } else {
            sql.push_str(token);
        }
        Ok(())
    }

    fn render_coercion(
        &self,
        sql: &mut String,
        coercion: &TypedCoercion,
    ) -> Result<(), SqlRenderError> {
        sql.push_str("::");
        self.render_type_name(sql, &coercion.target_type)?;
        if let Some(typmod) = &coercion.target_typmod {
            let value = typmod.as_str();
            if !valid_typmod(value) {
                return Err(SqlRenderError::InvalidTypmod);
            }
            sql.push_str(value);
        }
        Ok(())
    }

    fn render_window_spec(
        &mut self,
        sql: &mut String,
        specification: &'a WindowSpec<TypedExpression>,
    ) -> Result<(), SqlRenderError> {
        let mut wrote = false;
        if let Some(existing) = &specification.existing {
            quote_identifier(sql, existing);
            wrote = true;
        }
        if !specification.partition_by.is_empty() {
            space_if(sql, wrote);
            sql.push_str("PARTITION BY ");
            self.render_expression_list(sql, &specification.partition_by)?;
            wrote = true;
        }
        if !specification.order_by.is_empty() {
            space_if(sql, wrote);
            sql.push_str("ORDER BY ");
            self.render_ordering(sql, &specification.order_by)?;
            wrote = true;
        }
        if let Some(frame) = &specification.frame {
            space_if(sql, wrote);
            self.render_frame(sql, frame)?;
        }
        Ok(())
    }

    fn render_frame(
        &mut self,
        sql: &mut String,
        frame: &'a WindowFrame<TypedExpression>,
    ) -> Result<(), SqlRenderError> {
        sql.push_str(match frame.mode {
            WindowFrameMode::Rows => "ROWS ",
            WindowFrameMode::Range => "RANGE ",
            WindowFrameMode::Groups => "GROUPS ",
        });
        if let Some(end) = &frame.end {
            sql.push_str("BETWEEN ");
            self.render_frame_bound(sql, &frame.start)?;
            sql.push_str(" AND ");
            self.render_frame_bound(sql, end)?;
        } else {
            self.render_frame_bound(sql, &frame.start)?;
        }
        match frame.exclusion {
            WindowExclusion::None => {}
            WindowExclusion::CurrentRow => sql.push_str(" EXCLUDE CURRENT ROW"),
            WindowExclusion::Group => sql.push_str(" EXCLUDE GROUP"),
            WindowExclusion::Ties => sql.push_str(" EXCLUDE TIES"),
            WindowExclusion::NoOthers => sql.push_str(" EXCLUDE NO OTHERS"),
        }
        Ok(())
    }

    fn render_frame_bound(
        &mut self,
        sql: &mut String,
        bound: &'a FrameBound<TypedExpression>,
    ) -> Result<(), SqlRenderError> {
        match bound {
            FrameBound::UnboundedPreceding => sql.push_str("UNBOUNDED PRECEDING"),
            FrameBound::Preceding(expression) => {
                self.render_expression(sql, expression)?;
                sql.push_str(" PRECEDING");
            }
            FrameBound::CurrentRow => sql.push_str("CURRENT ROW"),
            FrameBound::Following(expression) => {
                self.render_expression(sql, expression)?;
                sql.push_str(" FOLLOWING");
            }
            FrameBound::UnboundedFollowing => sql.push_str("UNBOUNDED FOLLOWING"),
        }
        Ok(())
    }

    fn render_order_by_clause(
        &mut self,
        sql: &mut String,
        ordering: &'a [TypedOrderBy],
    ) -> Result<(), SqlRenderError> {
        if !ordering.is_empty() {
            sql.push_str(" ORDER BY ");
            self.render_ordering(sql, ordering)?;
        }
        Ok(())
    }

    fn render_ordering(
        &mut self,
        sql: &mut String,
        ordering: &'a [TypedOrderBy],
    ) -> Result<(), SqlRenderError> {
        for (index, term) in ordering.iter().enumerate() {
            comma(sql, index);
            self.render_expression(sql, &term.expression)?;
            sql.push_str(match term.direction {
                SortDirection::Ascending => " ASC",
                SortDirection::Descending => " DESC",
            });
            match term.nulls {
                NullsOrder::Default => {}
                NullsOrder::First => sql.push_str(" NULLS FIRST"),
                NullsOrder::Last => sql.push_str(" NULLS LAST"),
            }
        }
        Ok(())
    }

    fn render_limit(&mut self, sql: &mut String, limit: &TypedLimit) -> Result<(), SqlRenderError> {
        match limit {
            TypedLimit::Constant(value) => write!(sql, "{value}").unwrap(),
            TypedLimit::Parameter(parameter_id) => self.render_parameter(sql, *parameter_id)?,
        }
        Ok(())
    }

    fn render_lock(
        &mut self,
        sql: &mut String,
        lock: &HirLockClause,
    ) -> Result<(), SqlRenderError> {
        sql.push_str(match lock.strength {
            LockStrength::Update => " FOR UPDATE",
            LockStrength::NoKeyUpdate => " FOR NO KEY UPDATE",
            LockStrength::Share => " FOR SHARE",
            LockStrength::KeyShare => " FOR KEY SHARE",
        });
        if !lock.targets.is_empty() {
            sql.push_str(" OF ");
            for (index, relation) in lock.targets.iter().enumerate() {
                comma(sql, index);
                let qualifier = self.relation_qualifier(*relation)?;
                quote_identifier(sql, &qualifier);
            }
        }
        match lock.wait {
            LockWaitPolicy::Wait => {}
            LockWaitPolicy::NoWait => sql.push_str(" NOWAIT"),
            LockWaitPolicy::SkipLocked => sql.push_str(" SKIP LOCKED"),
        }
        Ok(())
    }

    fn render_conflict(
        &mut self,
        sql: &mut String,
        conflict: &'a TypedConflictClause,
    ) -> Result<(), SqlRenderError> {
        sql.push_str(" ON CONFLICT");
        match &conflict.target {
            ConflictTarget::Constraint(constraint_id) => {
                sql.push_str(" ON CONSTRAINT ");
                self.render_constraint_name(sql, constraint_id)?;
            }
            ConflictTarget::Inference {
                expressions,
                predicate,
            } => {
                if expressions.is_empty() {
                    return Err(SqlRenderError::InvalidArtifact("empty conflict inference"));
                }
                self.conflict_target_depth += 1;
                let render_result = (|| {
                    sql.push_str(" (");
                    self.render_expression_list(sql, expressions)?;
                    sql.push(')');
                    if let Some(predicate) = predicate {
                        sql.push_str(" WHERE ");
                        self.render_expression(sql, predicate)?;
                    }
                    Ok(())
                })();
                self.conflict_target_depth -= 1;
                render_result?;
            }
            ConflictTarget::Unspecified => {}
        }
        match &conflict.action {
            TypedConflictAction::Nothing => sql.push_str(" DO NOTHING"),
            TypedConflictAction::Update {
                assignments,
                predicate,
            } => {
                sql.push_str(" DO UPDATE SET ");
                self.render_assignments(sql, assignments)?;
                if let Some(predicate) = predicate {
                    sql.push_str(" WHERE ");
                    self.render_expression(sql, predicate)?;
                }
            }
        }
        Ok(())
    }

    fn render_assignments(
        &mut self,
        sql: &mut String,
        assignments: &'a [TypedAssignment],
    ) -> Result<(), SqlRenderError> {
        if assignments.is_empty() {
            return Err(SqlRenderError::InvalidArtifact("empty assignment list"));
        }
        for (index, assignment) in assignments.iter().enumerate() {
            comma(sql, index);
            self.render_column_name(sql, &assignment.target)?;
            sql.push_str(" = ");
            self.render_expression(sql, &assignment.value)?;
            if let Some(coercion) = &assignment.coercion {
                self.render_coercion(sql, coercion)?;
            }
        }
        Ok(())
    }

    fn push_ctes(&mut self, ctes: &'a [TypedCte]) {
        let mut scope = BTreeMap::new();
        for cte in ctes {
            scope.insert(
                cte.id,
                CteRender {
                    name: cte.name(),
                    fields: cte.output_fields(),
                    output_names: cte.output_names(),
                },
            );
        }
        self.cte_scopes.push(scope);
    }

    fn cte(&self, id: &CteId) -> Result<CteRender<'_>, SqlRenderError> {
        self.cte_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(id).copied())
            .ok_or(SqlRenderError::UnknownCte(*id))
    }

    fn push_relations(&mut self, relations: &[TypedRelation]) -> Result<(), SqlRenderError> {
        self.relation_scopes.push(BTreeMap::new());
        for relation in relations {
            self.register_relation_tree(relation)?;
        }
        Ok(())
    }

    fn push_target_relation(&mut self, table: TableId, binding: Option<RelationId>) {
        let mut scope = BTreeMap::new();
        let mut targets = BTreeSet::new();
        if let Some(binding) = binding {
            scope.insert(binding, table.as_str().to_string());
            targets.insert(binding);
        }
        self.relation_scopes.push(scope);
        self.target_scopes.push(targets);
    }

    fn register_relation_tree(&mut self, relation: &TypedRelation) -> Result<(), SqlRenderError> {
        let qualifier = match &relation.alias {
            Some(alias) => alias.name.clone(),
            None => match &relation.kind {
                TypedRelationKind::Table { table_id } => self
                    .query
                    .catalog_render_names
                    .table(table_id)
                    .and_then(|parts| parts.last())
                    .cloned()
                    .ok_or_else(|| {
                        SqlRenderError::MissingCatalogName(table_id.as_str().to_string())
                    })?,
                TypedRelationKind::Cte { cte_id } => self.cte(cte_id)?.name.to_string(),
                TypedRelationKind::Join { .. } => String::new(),
                _ => {
                    return Err(SqlRenderError::InvalidArtifact(
                        "derived relation requires an authored alias",
                    ));
                }
            },
        };
        if !qualifier.is_empty() {
            self.relation_scopes
                .last_mut()
                .ok_or(SqlRenderError::InvalidArtifact("missing relation scope"))?
                .insert(relation.id, qualifier);
        }
        if let TypedRelationKind::Join { left, right, .. } = &relation.kind {
            self.register_relation_tree(left)?;
            self.register_relation_tree(right)?;
        }
        Ok(())
    }

    fn relation_qualifier(&self, id: RelationId) -> Result<String, SqlRenderError> {
        self.relation_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&id).cloned())
            .ok_or(SqlRenderError::UnknownRelation(id))
    }

    fn is_target_binding(&self, id: RelationId) -> bool {
        self.target_scopes
            .iter()
            .rev()
            .any(|scope| scope.contains(&id))
    }

    fn target_table(&self) -> Result<TableId, SqlRenderError> {
        let id = self
            .target_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.iter().next())
            .ok_or(SqlRenderError::InvalidArtifact("missing target scope"))?;
        let raw = self
            .relation_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(id))
            .ok_or(SqlRenderError::UnknownRelation(*id))?;
        Ok(TableId::new(raw.clone()))
    }

    fn in_conflict_target(&self) -> bool {
        self.conflict_target_depth != 0
    }

    fn render_table_name(&self, sql: &mut String, id: &TableId) -> Result<(), SqlRenderError> {
        let components = self
            .query
            .catalog_render_names
            .table(id)
            .ok_or_else(|| SqlRenderError::MissingCatalogName(id.as_str().to_string()))?;
        render_qualified_identifier(sql, components);
        Ok(())
    }

    fn render_column_name(&self, sql: &mut String, id: &ColumnId) -> Result<(), SqlRenderError> {
        let name = self
            .query
            .catalog_render_names
            .column(id)
            .ok_or_else(|| SqlRenderError::MissingCatalogName(id.as_str().to_string()))?;
        quote_identifier(sql, name);
        Ok(())
    }

    fn render_callable_name(
        &self,
        sql: &mut String,
        id: &CallableId,
    ) -> Result<(), SqlRenderError> {
        let components = self
            .query
            .catalog_render_names
            .callable(id)
            .ok_or_else(|| SqlRenderError::MissingCatalogName(id.as_str().to_string()))?;
        render_qualified_identifier(sql, components);
        Ok(())
    }

    fn render_type_name(&self, sql: &mut String, id: &TypeId) -> Result<(), SqlRenderError> {
        let components = self
            .query
            .catalog_render_names
            .type_name(id)
            .ok_or_else(|| SqlRenderError::MissingCatalogName(id.as_str().to_string()))?;
        render_qualified_identifier(sql, components);
        Ok(())
    }

    fn render_collation_name(
        &self,
        sql: &mut String,
        id: &CollationId,
    ) -> Result<(), SqlRenderError> {
        let components = self
            .query
            .catalog_render_names
            .collation(id)
            .ok_or_else(|| SqlRenderError::MissingCatalogName(id.as_str().to_string()))?;
        render_qualified_identifier(sql, components);
        Ok(())
    }

    fn render_constraint_name(
        &self,
        sql: &mut String,
        id: &ConstraintId,
    ) -> Result<(), SqlRenderError> {
        let name = self
            .query
            .catalog_render_names
            .constraint(id)
            .ok_or_else(|| SqlRenderError::MissingCatalogName(id.as_str().to_string()))?;
        quote_identifier(sql, name);
        Ok(())
    }
}

fn split_operator(components: &[String]) -> Result<(Option<&str>, &str), SqlRenderError> {
    match components {
        [token] => Ok((None, token)),
        [schema, token] => Ok((Some(schema), token)),
        _ => Err(SqlRenderError::InvalidOperator(components.join("."))),
    }
}

fn prefix_operator(token: &str) -> bool {
    matches!(token, "NOT" | "+" | "-" | "~" | "@" | "!!")
}

fn symbolic_operator(token: &str) -> bool {
    token
        .bytes()
        .any(|byte| b"+-*/<>=~!@#%^&|`?".contains(&byte))
}

fn valid_operator_token(token: &str) -> bool {
    if token.is_empty() || token.contains('\0') || token.contains(';') || token.contains("--") {
        return false;
    }
    if symbolic_operator(token) {
        return token
            .bytes()
            .all(|byte| b"+-*/<>=~!@#%^&|`?".contains(&byte));
    }
    token.split(' ').all(|word| {
        !word.is_empty()
            && word
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte == b'_')
    })
}

fn valid_integer(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_numeric(value: &str) -> bool {
    if value.is_empty() || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return false;
    }
    let mut chars = value.chars().peekable();
    if matches!(chars.peek(), Some('+') | Some('-')) {
        chars.next();
    }
    let mut digits = 0usize;
    let mut decimal = false;
    let mut exponent = false;
    let mut exponent_digits = 0usize;
    while let Some(character) = chars.next() {
        match character {
            '0'..='9' => {
                if exponent {
                    exponent_digits += 1;
                } else {
                    digits += 1;
                }
            }
            '.' if !decimal && !exponent => decimal = true,
            'e' | 'E' if !exponent && digits > 0 => {
                exponent = true;
                if matches!(chars.peek(), Some('+') | Some('-')) {
                    chars.next();
                }
            }
            _ => return false,
        }
    }
    digits > 0 && (!exponent || exponent_digits > 0)
}

fn valid_typmod(value: &str) -> bool {
    if value.is_empty() || value.contains('\0') || value.contains(';') || value.contains("--") {
        return false;
    }
    let Some(open) = value.find('(') else {
        return value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b' ');
    };
    value.ends_with(')')
        && value[..open]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b' ')
        && value[open + 1..value.len() - 1]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b',' || byte == b' ' || byte == b'-')
}

fn render_qualified_identifier(sql: &mut String, components: &[String]) {
    for (index, component) in components.iter().enumerate() {
        if index > 0 {
            sql.push('.');
        }
        quote_identifier(sql, component);
    }
}

fn quote_identifier(sql: &mut String, value: &str) {
    sql.push('"');
    for character in value.chars() {
        if character == '"' {
            sql.push('"');
        }
        sql.push(character);
    }
    sql.push('"');
}

fn quote_string(sql: &mut String, value: &str) {
    sql.push('\'');
    for character in value.chars() {
        if character == '\'' {
            sql.push('\'');
        }
        sql.push(character);
    }
    sql.push('\'');
}

fn comma(sql: &mut String, index: usize) {
    if index > 0 {
        sql.push_str(", ");
    }
}

fn space_if(sql: &mut String, condition: bool) {
    if condition {
        sql.push(' ');
    }
}
