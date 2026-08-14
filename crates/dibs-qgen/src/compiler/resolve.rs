use std::collections::{BTreeMap, BTreeSet};

use dibs_pg_catalog::{CallableId, CallableKind, CatalogSnapshot, OperatorId};
use dibs_query_ir::{
    AssignmentId, CteId, CteMaterialization, ExpressionId, ExtractField, FieldId, FrameBound,
    HirAssignment, HirCall, HirConflictAction, HirConflictClause, HirConflictTarget, HirCte,
    HirDelete, HirExpression, HirExpressionKind, HirInsert, HirInsertSource, HirLiteral,
    HirLockClause, HirNamedWindow, HirOrderBy, HirParameter, HirProjection, HirQuery, HirRelation,
    HirRelationKind, HirSelect, HirStatement, HirStatementKind, HirUpdate, HirValues, LockStrength,
    LockWaitPolicy, NullsOrder, ParameterId, QueryId, RelationAlias, RelationId, SelectDistinct,
    SetOperationKind, SortDirection, SourceOrigin, StatementId, WindowExclusion, WindowFrame,
    WindowFrameMode, WindowReference, WindowSpec,
};
use dibs_query_syntax::{
    SourceId, SourceSpan, Span, ast,
    ast::{
        AdditiveExpression, AndExpression, AtomExpression, ExponentExpression, Expression,
        GenericExpression, MultiplicativeExpression, NotExpression, OrExpression,
        ParenthesizedValue, PostfixExpression, PredicateExpression, Relation, Statement,
        UnaryExpression,
    },
};

use super::{CompileDiagnostic, CompileDiagnosticCode, DiagnosticSet};
use crate::compiler::scope::{
    CteBinding, ProjectionBinding, RelationBinding, RelationColumnBinding, RelationFieldBinding,
    SelectScope,
};

#[derive(Debug)]
pub(crate) struct ResolvedQuery {
    pub(crate) hir: HirQuery,
}

pub(crate) fn resolve_file(
    source_id: SourceId,
    file: ast::SourceFile,
    catalog: &CatalogSnapshot,
) -> Result<Vec<ResolvedQuery>, DiagnosticSet> {
    if catalog.postgres_major != 18 {
        return Err(vec![CompileDiagnostic::new(
            CompileDiagnosticCode::InvalidArtifact,
            SourceSpan::new(source_id, file.span),
            format!(
                "Dibs query compilation requires PostgreSQL 18, got {}",
                catalog.postgres_major
            ),
        )]);
    }

    let mut output = Vec::with_capacity(file.queries.len());
    for (index, query) in file.queries.into_iter().enumerate() {
        let query_id = QueryId::new(u32::try_from(index + 1).map_err(|_| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::InvalidArtifact,
                SourceSpan::new(source_id, query.span),
                "too many query declarations",
            )]
        })?);
        output.push(Resolver::new(source_id, catalog, query_id).resolve_query(query)?);
    }
    Ok(output)
}

struct Resolver<'catalog> {
    source_id: SourceId,
    catalog: &'catalog CatalogSnapshot,
    query_id: QueryId,
    next_statement: u32,
    next_cte: u32,
    next_relation: u32,
    next_expression: u32,
    next_field: u32,
    next_assignment: u32,
    parameters: BTreeMap<String, HirParameter>,
    used_parameters: BTreeSet<ParameterId>,
    referenced_ctes: BTreeSet<CteId>,
}

impl<'catalog> Resolver<'catalog> {
    fn new(source_id: SourceId, catalog: &'catalog CatalogSnapshot, query_id: QueryId) -> Self {
        Self {
            source_id,
            catalog,
            query_id,
            next_statement: 1,
            next_cte: 1,
            next_relation: 1,
            next_expression: 1,
            next_assignment: 1,
            next_field: 1,
            parameters: BTreeMap::new(),
            used_parameters: BTreeSet::new(),
            referenced_ctes: BTreeSet::new(),
        }
    }

    fn resolve_query(mut self, query: ast::QueryDecl) -> Result<ResolvedQuery, DiagnosticSet> {
        let query_origin = self.origin(query.span);
        let mut ordered_parameters = Vec::with_capacity(query.parameters.len());
        for (ordinal, parameter) in query.parameters.into_iter().enumerate() {
            let name = parameter.name.value.clone();
            if let Some(existing) = self.parameters.get(&name) {
                return Err(vec![
                    CompileDiagnostic::new(
                        CompileDiagnosticCode::DuplicateParameter,
                        self.source_span(parameter.name.span),
                        format!("parameter '{name}' is declared more than once"),
                    )
                    .with_related(vec![existing.origin.span()]),
                ]);
            }
            let (type_id, typmod) = self.resolve_parameter_type(&parameter.type_name)?;
            let resolved = HirParameter {
                id: ParameterId::new(u32::try_from(ordinal + 1).map_err(|_| {
                    vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::InvalidArtifact,
                        self.source_span(parameter.span),
                        "too many parameters",
                    )]
                })?),
                ordinal: u32::try_from(ordinal).map_err(|_| {
                    vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::InvalidArtifact,
                        self.source_span(parameter.span),
                        "too many parameters",
                    )]
                })?,
                name: name.clone(),
                origin: self.origin(parameter.span),
                type_id,
                typmod,
                nullable: parameter.nullable,
            };
            self.parameters.insert(name, resolved.clone());
            ordered_parameters.push(resolved);
        }

        let statement = self.resolve_statement(query.statement)?;
        for parameter in &ordered_parameters {
            if !self.used_parameters.contains(&parameter.id) {
                return Err(vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::UnusedParameter,
                    parameter.origin.span(),
                    format!("parameter '{}' is never used", parameter.name),
                )]);
            }
        }

        Ok(ResolvedQuery {
            hir: HirQuery {
                id: self.query_id,
                name: query.name.value,
                origin: query_origin,
                parameters: ordered_parameters,
                statement,
            },
        })
    }

    fn resolve_parameter_type(
        &self,
        type_name: &ast::PgTypeName,
    ) -> Result<(dibs_pg_catalog::TypeId, Option<dibs_query_ir::Typmod>), DiagnosticSet> {
        let base = match &type_name.schema {
            Some(schema) => format!("{}.{}", schema.value, type_name.name.value),
            None => format!(
                "pg_catalog.{}",
                canonical_builtin_type_name(&type_name.name.value)
            ),
        };
        let qualified = format!("{base}{}", "[]".repeat(type_name.arraies.len()));
        let ty = self.catalog.resolve_type(&qualified).map_err(|error| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::TypeMismatch,
                self.source_span(type_name.span),
                error.to_string(),
            )]
        })?;
        let typmod = type_name.typmod.as_ref().map(|modifier| {
            dibs_query_ir::Typmod::new(
                modifier
                    .values
                    .iter()
                    .map(|value| value.value.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            )
        });
        Ok((ty.id.clone(), typmod))
    }

    fn resolve_statement(&mut self, statement: Statement) -> Result<HirStatement, DiagnosticSet> {
        self.resolve_statement_in_scope(statement, &SelectScope::default())
    }

    fn resolve_statement_in_scope(
        &mut self,
        statement: Statement,
        scope: &SelectScope,
    ) -> Result<HirStatement, DiagnosticSet> {
        let span = statement_span(&statement);
        let id = self.statement_id();
        let origin = self.origin(span);
        let kind =
            match statement {
                Statement::With(with) => {
                    return self.resolve_with_statement(*with, scope, id, origin);
                }
                Statement::Select(select) => HirStatementKind::Select(Box::new(
                    self.resolve_select(*select, Some(scope), None, None)?,
                )),
                Statement::Insert(insert) => {
                    HirStatementKind::Insert(Box::new(self.resolve_insert(*insert, scope)?))
                }
                Statement::Update(update) => HirStatementKind::Update(Box::new(
                    self.resolve_update(*update, Vec::new(), scope)?,
                )),
                Statement::Delete(delete) => HirStatementKind::Delete(Box::new(
                    self.resolve_delete(*delete, Vec::new(), scope)?,
                )),
                _ => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(span),
                        "statement compiler path does not yet accept this statement kind",
                    )]);
                }
            };
        Ok(HirStatement { id, origin, kind })
    }

    fn resolve_with_statement(
        &mut self,
        with: ast::WithStatement,
        parent_scope: &SelectScope,
        id: StatementId,
        origin: SourceOrigin,
    ) -> Result<HirStatement, DiagnosticSet> {
        let recursive_list = with.with.recursive.is_some();
        let mut scope = SelectScope::with_parent(parent_scope);
        let mut ctes = Vec::with_capacity(with.with.ctes.len());
        for cte in with.with.ctes {
            let ast::CommonTableExpr {
                span,
                name,
                columns,
                materialization,
                statement: cte_statement,
            } = cte;
            let cte_id = self.cte_id();
            let cte_origin = self.origin(span);
            let materialization = match materialization
                .as_ref()
                .map(|value| compact_keyword(&value.value))
                .as_deref()
            {
                None => CteMaterialization::Default,
                Some("materialized") => CteMaterialization::Materialized,
                Some("notmaterialized") => CteMaterialization::NotMaterialized,
                Some(_) => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(span),
                        "unknown CTE materialization policy",
                    )]);
                }
            };
            let recursive_resolution = if recursive_list {
                self.resolve_recursive_cte_statement(
                    cte_statement.value.clone(),
                    cte_id,
                    &name.value,
                    columns.as_ref(),
                    &scope,
                )?
            } else {
                None
            };
            let (statement, recursive) = if let Some(resolved) = recursive_resolution {
                resolved
            } else {
                (
                    self.resolve_statement_in_scope(cte_statement.value, &scope)?,
                    false,
                )
            };
            let projections = statement_projections(&statement);
            let output_names =
                cte_output_names(columns.as_ref(), projections).map_err(|message| {
                    vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::TypeMismatch,
                        self.source_span(columns.as_ref().map_or(span, |columns| columns.span)),
                        message,
                    )]
                })?;
            let columns = projections
                .iter()
                .zip(output_names)
                .map(|(projection, name)| RelationColumnBinding {
                    name,
                    field: RelationFieldBinding::Cte {
                        cte_id,
                        field_id: projection.field_id,
                    },
                })
                .collect();
            scope
                .insert_cte(
                    name.value.clone(),
                    CteBinding {
                        id: cte_id,
                        columns,
                        origin: cte_origin.clone(),
                    },
                )
                .map_err(|diagnostic| vec![diagnostic])?;
            ctes.push(HirCte {
                id: cte_id,
                recursive,
                name: name.value,
                origin: cte_origin,
                materialization,
                statement: Box::new(statement),
            });
        }

        let mut statement = self.resolve_statement_in_scope(with.body.value, &scope)?;
        match &mut statement.kind {
            HirStatementKind::Select(select) => {
                select.recursive = recursive_list;
                select.ctes = ctes;
            }
            HirStatementKind::Insert(insert) if !recursive_list => insert.ctes = ctes,
            HirStatementKind::Update(update) if !recursive_list => update.ctes = ctes,
            HirStatementKind::Delete(delete) if !recursive_list => delete.ctes = ctes,
            _ => {
                return Err(vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::UnsupportedClause,
                    origin.span(),
                    "WITH RECURSIVE currently requires a SELECT body",
                )]);
            }
        }
        statement.id = id;
        statement.origin = origin;
        Ok(statement)
    }

    fn resolve_recursive_cte_statement(
        &mut self,
        statement: Statement,
        cte_id: CteId,
        cte_name: &str,
        columns: Option<&ast::ColumnNameList>,
        scope: &SelectScope,
    ) -> Result<Option<(HirStatement, bool)>, DiagnosticSet> {
        let Statement::Select(select) = statement else {
            return Ok(None);
        };
        if select.order_by.is_some()
            || !select.locks.is_empty()
            || select.limit.is_some()
            || select.offset.is_some()
            || select.fetch.is_some()
        {
            return Ok(None);
        }
        let ast::SelectSetNode::SetOperation(operation) = select.body.value else {
            return Ok(None);
        };
        let ast::SetOperation {
            span,
            left,
            operator,
            quantifier,
            right,
        } = *operation;
        let ast::SetOperator::UnionOrExcept(operator) = operator else {
            return Ok(None);
        };
        if compact_keyword(&operator.value) != "union" {
            return Ok(None);
        }
        let left = self.resolve_set_operand(left, scope, None, None)?;
        let anchor = statement_projections(&left);
        let output_names = cte_output_names(columns, anchor).map_err(|message| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::TypeMismatch,
                self.source_span(columns.map_or(span, |columns| columns.span)),
                message,
            )]
        })?;
        let wrapper_fields = (0..anchor.len())
            .map(|_| self.field_id())
            .collect::<Vec<_>>();
        let mut recursive_scope = scope.clone();
        recursive_scope
            .insert_cte(
                cte_name.to_string(),
                CteBinding {
                    id: cte_id,
                    columns: wrapper_fields
                        .iter()
                        .copied()
                        .zip(output_names.iter().cloned())
                        .map(|(field_id, name)| RelationColumnBinding {
                            name,
                            field: RelationFieldBinding::Cte { cte_id, field_id },
                        })
                        .collect(),
                    origin: self.origin(span),
                },
            )
            .map_err(|diagnostic| vec![diagnostic])?;
        let previously_referenced = self.referenced_ctes.remove(&cte_id);
        let right = self.resolve_set_operand(right, &recursive_scope, Some(&output_names), None)?;
        let recursive = self.referenced_ctes.remove(&cte_id);
        if previously_referenced {
            self.referenced_ctes.insert(cte_id);
        }
        let all = quantifier
            .as_ref()
            .is_some_and(|value| compact_keyword(&value.value) == "all");
        let select = self.build_set_select(
            left,
            right,
            SetOperationKind::Union,
            all,
            span,
            wrapper_fields,
            output_names,
        );
        Ok(Some((
            HirStatement {
                id: self.statement_id(),
                origin: self.origin(span),
                kind: HirStatementKind::Select(Box::new(select)),
            },
            recursive,
        )))
    }

    fn resolve_insert(
        &mut self,
        insert: ast::InsertStatement,
        enclosing_scope: &SelectScope,
    ) -> Result<HirInsert, DiagnosticSet> {
        let authored_name = qualified_name(&insert.target.name);
        let table = resolve_table_name(self.catalog, &authored_name).map_err(|code| {
            vec![CompileDiagnostic::new(
                code,
                self.source_span(insert.target.name.span),
                format!("unknown or ambiguous relation '{authored_name}'"),
            )]
        })?;
        let target_binding = self.relation_id();
        let excluded_binding = self.relation_id();
        let visible_name = insert.target.alias.as_ref().map_or_else(
            || {
                insert
                    .target
                    .name
                    .parts
                    .last()
                    .expect("qualified name")
                    .value
                    .clone()
            },
            |alias| alias.name.value.clone(),
        );
        let target_scope_binding =
            self.table_binding(table, target_binding, visible_name, insert.target.span);
        let excluded_scope_binding = self.table_binding(
            table,
            excluded_binding,
            "excluded".to_string(),
            insert.target.span,
        );
        let source_scope = SelectScope::with_parent(enclosing_scope);
        let columns = insert
            .target
            .columns
            .as_ref()
            .ok_or_else(|| {
                vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::UnsupportedClause,
                    self.source_span(insert.target.span),
                    "INSERT requires an explicit target column list",
                )]
            })?
            .columns
            .iter()
            .map(|column| {
                table
                    .column(&column.value)
                    .map(|column| column.id.clone())
                    .ok_or_else(|| {
                        vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnknownField,
                            self.source_span(column.span),
                            format!(
                                "table '{}' has no field '{}'",
                                table.qualified_name, column.value
                            ),
                        )]
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let source = match insert.source.value {
            ast::InsertSourceNode::DefaultValuesClause(_) => HirInsertSource::DefaultValues,
            ast::InsertSourceNode::Values(values) => {
                let rows = values
                    .rows
                    .iter()
                    .map(|row| {
                        row.values
                            .iter()
                            .map(|value| match &value.value {
                                ast::InsertValueNode::Expression(expression) => {
                                    self.resolve_expression(expression, &source_scope)
                                }
                                ast::InsertValueNode::DefaultLiteral(default) => {
                                    Err(vec![CompileDiagnostic::new(
                                        CompileDiagnosticCode::UnsupportedClause,
                                        self.source_span(default.span),
                                        "DEFAULT inside VALUES is not yet supported",
                                    )])
                                }
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if rows.iter().any(|row| row.len() != columns.len()) {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::TypeMismatch,
                        self.source_span(values.span),
                        "every INSERT VALUES row must match the target column count",
                    )]);
                }
                HirInsertSource::Values(HirValues::try_new(rows).map_err(|error| {
                    vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::TypeMismatch,
                        self.source_span(values.span),
                        error.to_string(),
                    )]
                })?)
            }
            ast::InsertSourceNode::With(with) => HirInsertSource::Select(Box::new(
                self.resolve_statement_in_scope(Statement::With(with), &source_scope)?,
            )),
            ast::InsertSourceNode::Query(query) => {
                let ast::InsertQuerySource {
                    span,
                    body,
                    order_by,
                    locks,
                    limit,
                    offset,
                    fetch,
                } = *query;
                let body_span = match &body {
                    ast::InsertQueryBody::SelectCore(body) => body.span,
                    ast::InsertQueryBody::TableQuery(body) => body.span,
                    ast::InsertQueryBody::ParenthesizedQuery(body) => body.span,
                    ast::InsertQueryBody::SetOperation(body) => body.span,
                };
                let body = match body {
                    ast::InsertQueryBody::SelectCore(body) => ast::SelectSetNode::SelectCore(body),
                    ast::InsertQueryBody::TableQuery(body) => ast::SelectSetNode::TableQuery(body),
                    ast::InsertQueryBody::ParenthesizedQuery(body) => {
                        ast::SelectSetNode::ParenthesizedQuery(body)
                    }
                    ast::InsertQueryBody::SetOperation(body) => {
                        ast::SelectSetNode::SetOperation(body)
                    }
                };
                let statement = Statement::Select(Box::new(ast::SelectStatement {
                    span,
                    body: ast::SelectSetExpression {
                        span: body_span,
                        value: body,
                    },
                    order_by,
                    locks,
                    limit,
                    offset,
                    fetch,
                }));
                HirInsertSource::Select(Box::new(self.resolve_nested_statement(
                    statement,
                    &source_scope,
                    Some("__dibs_insert"),
                )?))
            }
        };
        let mut target_scope = SelectScope::default();
        target_scope
            .insert_relation(target_scope_binding.clone())
            .map_err(|error| vec![error])?;
        let mut action_scope = target_scope.clone();
        action_scope
            .insert_relation(excluded_scope_binding)
            .map_err(|error| vec![error])?;
        let conflict = insert
            .conflict
            .map(|conflict| {
                self.resolve_conflict(
                    conflict,
                    table,
                    excluded_binding,
                    &target_scope,
                    &action_scope,
                )
            })
            .transpose()?;
        let returning = insert
            .returning
            .map(|returning| {
                self.resolve_projections(
                    returning.targets,
                    &mut target_scope,
                    "RETURNING",
                    None,
                    None,
                )
            })
            .transpose()?
            .unwrap_or_default();
        Ok(HirInsert {
            ctes: Vec::new(),
            target: table.id.clone(),
            target_binding,
            columns,
            source,
            conflict,
            returning,
        })
    }

    fn resolve_update(
        &mut self,
        update: ast::UpdateStatement,
        ctes: Vec<HirCte>,
        enclosing_scope: &SelectScope,
    ) -> Result<HirUpdate, DiagnosticSet> {
        let authored_name = qualified_name(&update.target);
        let table = resolve_table_name(self.catalog, &authored_name).map_err(|code| {
            vec![CompileDiagnostic::new(
                code,
                self.source_span(update.target.span),
                format!("unknown or ambiguous relation '{authored_name}'"),
            )]
        })?;
        let target_binding = self.relation_id();
        let visible_name = update.alias.as_ref().map_or_else(
            || {
                update
                    .target
                    .parts
                    .last()
                    .expect("qualified name")
                    .value
                    .clone()
            },
            |alias| alias.name.value.clone(),
        );
        let mut scope = SelectScope::with_parent(enclosing_scope);
        scope
            .insert_relation(self.table_binding(
                table,
                target_binding,
                visible_name,
                update.target.span,
            ))
            .map_err(|diagnostic| vec![diagnostic])?;

        let mut from = Vec::new();
        if let Some(from_clause) = update.from {
            for relation in from_clause.relations {
                let (hir, bindings, lateral) = self.resolve_relation(relation, &scope)?;
                if lateral {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        hir.origin.span(),
                        "LATERAL UPDATE FROM input is not available yet",
                    )]);
                }
                scope
                    .extend_relations(bindings)
                    .map_err(|diagnostic| vec![diagnostic])?;
                from.push(hir);
            }
        }
        let assignments = update
            .assignments
            .iter()
            .map(|assignment| self.resolve_assignment(assignment, table, &scope))
            .collect::<Result<Vec<_>, _>>()?;
        let predicate = update
            .r#where
            .as_ref()
            .map(|clause| self.resolve_expression(&clause.expression, &scope))
            .transpose()?;
        let returning = update
            .returning
            .map(|returning| {
                self.resolve_projections(returning.targets, &mut scope, "RETURNING", None, None)
            })
            .transpose()?
            .unwrap_or_default();
        Ok(HirUpdate {
            ctes,
            target: table.id.clone(),
            target_binding,
            assignments,
            from,
            predicate,
            returning,
        })
    }

    fn resolve_delete(
        &mut self,
        delete: ast::DeleteStatement,
        ctes: Vec<HirCte>,
        enclosing_scope: &SelectScope,
    ) -> Result<HirDelete, DiagnosticSet> {
        let authored_name = qualified_name(&delete.target);
        let table = resolve_table_name(self.catalog, &authored_name).map_err(|code| {
            vec![CompileDiagnostic::new(
                code,
                self.source_span(delete.target.span),
                format!("unknown or ambiguous relation '{authored_name}'"),
            )]
        })?;
        let target_binding = self.relation_id();
        let visible_name = delete.alias.as_ref().map_or_else(
            || {
                delete
                    .target
                    .parts
                    .last()
                    .expect("qualified name")
                    .value
                    .clone()
            },
            |alias| alias.name.value.clone(),
        );
        let mut scope = SelectScope::with_parent(enclosing_scope);
        scope
            .insert_relation(self.table_binding(
                table,
                target_binding,
                visible_name,
                delete.target.span,
            ))
            .map_err(|diagnostic| vec![diagnostic])?;

        let mut using_relations = Vec::new();
        if let Some(using) = delete.using {
            for relation in using.relations {
                let (hir, bindings, lateral) = self.resolve_relation(relation, &scope)?;
                if lateral {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        hir.origin.span(),
                        "LATERAL DELETE USING input is not available yet",
                    )]);
                }
                scope
                    .extend_relations(bindings)
                    .map_err(|diagnostic| vec![diagnostic])?;
                using_relations.push(hir);
            }
        }
        let predicate = delete
            .r#where
            .as_ref()
            .map(|clause| self.resolve_expression(&clause.expression, &scope))
            .transpose()?;
        let returning = delete
            .returning
            .map(|returning| {
                self.resolve_projections(returning.targets, &mut scope, "RETURNING", None, None)
            })
            .transpose()?
            .unwrap_or_default();
        Ok(HirDelete {
            ctes,
            target: table.id.clone(),
            target_binding,
            using_relations,
            predicate,
            returning,
        })
    }

    fn resolve_conflict(
        &mut self,
        conflict: ast::ConflictClause,
        table: &dibs_pg_catalog::CatalogTable,
        excluded_binding: RelationId,
        target_scope: &SelectScope,
        action_scope: &SelectScope,
    ) -> Result<HirConflictClause, DiagnosticSet> {
        let target = match conflict.target.map(|target| target.value) {
            None => HirConflictTarget::Unspecified,
            Some(ast::ConflictTargetNode::ConflictInference(inference)) => {
                if inference
                    .elements
                    .iter()
                    .any(|element| element.collation.is_some() || element.operator_class.is_some())
                {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(inference.span),
                        "conflict target collation and operator classes are not yet supported",
                    )]);
                }
                HirConflictTarget::Inference {
                    expressions: inference
                        .elements
                        .iter()
                        .map(|element| self.resolve_expression(&element.expression, target_scope))
                        .collect::<Result<Vec<_>, _>>()?,
                    predicate: inference
                        .predicate
                        .as_ref()
                        .map(|predicate| {
                            self.resolve_expression(&predicate.expression, target_scope)
                                .map(Box::new)
                        })
                        .transpose()?,
                }
            }
            Some(ast::ConflictTargetNode::ConflictConstraint(constraint)) => {
                let name = qualified_name(&constraint.constraint);
                let resolved = table
                    .unique_constraints
                    .iter()
                    .find(|candidate| candidate.name == name)
                    .ok_or_else(|| {
                        vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnknownField,
                            self.source_span(constraint.span),
                            format!(
                                "table '{}' has no unique constraint '{name}'",
                                table.qualified_name
                            ),
                        )]
                    })?;
                HirConflictTarget::Constraint(resolved.id.clone())
            }
        };
        let action = match conflict.action.value {
            ast::ConflictActionNode::ConflictDoNothing(_) => HirConflictAction::Nothing,
            ast::ConflictActionNode::ConflictDoUpdate(update) => HirConflictAction::Update {
                assignments: update
                    .assignments
                    .iter()
                    .map(|assignment| self.resolve_assignment(assignment, table, action_scope))
                    .collect::<Result<Vec<_>, _>>()?,
                predicate: update
                    .predicate
                    .as_ref()
                    .map(|predicate| self.resolve_expression(&predicate.expression, action_scope))
                    .transpose()?,
            },
        };
        Ok(HirConflictClause {
            target,
            excluded_binding,
            action,
        })
    }

    fn resolve_assignment(
        &mut self,
        assignment: &ast::Assignment,
        table: &dibs_pg_catalog::CatalogTable,
        scope: &SelectScope,
    ) -> Result<HirAssignment, DiagnosticSet> {
        if assignment.targets.is_some() {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(assignment.span),
                "row assignments are not yet supported",
            )]);
        }
        let target = assignment.target.as_ref().ok_or_else(|| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnknownField,
                self.source_span(assignment.span),
                "assignment target is missing",
            )]
        })?;
        if !target.indirections.is_empty() {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(target.span),
                "assignment indirection is not yet supported",
            )]);
        }
        let column = table.column(&target.name.value).ok_or_else(|| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnknownField,
                self.source_span(target.name.span),
                format!(
                    "table '{}' has no field '{}'",
                    table.qualified_name, target.name.value
                ),
            )]
        })?;
        let expression = match &assignment.value {
            ast::AssignmentValue::InsertValue(value) => match &value.value {
                ast::InsertValueNode::Expression(expression) => expression,
                ast::InsertValueNode::DefaultLiteral(default) => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(default.span),
                        "DEFAULT assignment is not yet supported",
                    )]);
                }
            },
            ast::AssignmentValue::Parenthesized(parenthesized) => match &parenthesized.value {
                ParenthesizedValue::Scalar(scalar) => &scalar.expression,
                _ => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(parenthesized.span),
                        "row and subquery assignments are not yet supported",
                    )]);
                }
            },
        };
        Ok(HirAssignment {
            id: self.assignment_id(),
            target: column.id.clone(),
            value: self.resolve_expression(expression, scope)?,
        })
    }

    fn table_binding(
        &self,
        table: &dibs_pg_catalog::CatalogTable,
        id: RelationId,
        visible_name: String,
        span: Span,
    ) -> RelationBinding {
        RelationBinding {
            id,
            columns: table
                .columns
                .iter()
                .map(|column| RelationColumnBinding {
                    name: column.name.clone(),
                    field: RelationFieldBinding::Catalog(column.id.clone()),
                })
                .collect(),
            origin: self.origin(span),
            visible_name,
        }
    }

    fn resolve_projections(
        &mut self,
        targets: Vec<ast::SelectTarget>,
        scope: &mut SelectScope,
        clause: &str,
        inherited_output_names: Option<&[String]>,
        synthetic_output_prefix: Option<&str>,
    ) -> Result<Vec<HirProjection>, DiagnosticSet> {
        let mut projections = Vec::with_capacity(targets.len());
        for target in targets {
            match target.value {
                ast::SelectTargetValue::Expression(target) => {
                    let expression = self.resolve_expression(&target.expression, scope)?;
                    let (alias, alias_span) = if let Some(alias) = target.alias {
                        (alias.name.value, alias.name.span)
                    } else if let Some((_, name, span)) = simple_column_name(&target.expression) {
                        (name.to_string(), span)
                    } else if let Some(alias) =
                        inherited_output_names.and_then(|names| names.get(projections.len()))
                    {
                        (alias.clone(), target.span)
                    } else if let Some(prefix) = synthetic_output_prefix {
                        (format!("{prefix}_{}", projections.len()), target.span)
                    } else {
                        return Err(vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::MissingOutputAlias,
                            self.source_span(target.span),
                            format!("computed {clause} expressions require an explicit alias"),
                        )]);
                    };
                    let field_id = self.field_id();
                    let projection = HirProjection {
                        field_id,
                        alias: alias.clone(),
                        alias_origin: self.origin(alias_span),
                        expression: expression.clone(),
                    };
                    scope
                        .insert_projection(ProjectionBinding {
                            alias,
                            expression,
                            origin: projection.alias_origin.clone(),
                        })
                        .map_err(|diagnostic| vec![diagnostic])?;
                    projections.push(projection);
                }
                ast::SelectTargetValue::Wildcard(span) => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(span),
                        format!("wildcard {clause} projections are not accepted"),
                    )]);
                }
                ast::SelectTargetValue::QualifiedWildcard(target) => {
                    let qualifier = qualified_name(&target.qualifier);
                    let binding = scope.relation(&qualifier).cloned().ok_or_else(|| {
                        vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnknownRelation,
                            self.source_span(target.qualifier.span),
                            format!("unknown relation '{qualifier}'"),
                        )]
                    })?;
                    for column in binding.columns {
                        let expression = HirExpression {
                            id: self.expression_id(),
                            origin: self.origin(target.span),
                            kind: match column.field {
                                RelationFieldBinding::Catalog(column_id) => {
                                    HirExpressionKind::Column {
                                        binding: binding.id,
                                        column_id,
                                    }
                                }
                                RelationFieldBinding::Derived(field_id) => {
                                    HirExpressionKind::DerivedColumn {
                                        binding: binding.id,
                                        field_id,
                                    }
                                }
                                RelationFieldBinding::Cte { cte_id, field_id } => {
                                    HirExpressionKind::CteColumn {
                                        cte_id,
                                        binding: binding.id,
                                        field_id,
                                    }
                                }
                            },
                        };
                        let field_id = self.field_id();
                        let projection = HirProjection {
                            field_id,
                            alias: column.name.clone(),
                            alias_origin: self.origin(target.span),
                            expression: expression.clone(),
                        };
                        scope
                            .insert_projection(ProjectionBinding {
                                alias: column.name,
                                expression,
                                origin: projection.alias_origin.clone(),
                            })
                            .map_err(|diagnostic| vec![diagnostic])?;
                        projections.push(projection);
                    }
                }
            }
        }
        Ok(projections)
    }

    fn resolve_select(
        &mut self,
        select: ast::SelectStatement,
        parent_scope: Option<&SelectScope>,
        inherited_output_names: Option<&[String]>,
        synthetic_output_prefix: Option<&str>,
    ) -> Result<HirSelect, DiagnosticSet> {
        if select.fetch.is_some() {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(select.span),
                "FETCH is outside the ordinary SELECT compiler path",
            )]);
        }
        if matches!(select.body.value, ast::SelectSetNode::SetOperation(_)) {
            return self.resolve_set_select(
                select,
                parent_scope,
                inherited_output_names,
                synthetic_output_prefix,
            );
        }
        let ast::SelectSetNode::SelectCore(core) = select.body.value else {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(select.body.span),
                "set operations, VALUES, and parenthesized query bodies are outside the ordinary SELECT compiler path",
            )]);
        };

        let mut scope = parent_scope.map_or_else(SelectScope::default, SelectScope::with_parent);
        let mut relations = Vec::new();
        if let Some(from) = core.from {
            for relation in from.relations {
                let (hir, bindings, lateral) = self.resolve_relation(relation, &scope)?;
                scope
                    .extend_relations(bindings)
                    .map_err(|diagnostic| vec![diagnostic])?;
                if lateral {
                    let left = relations.pop().ok_or_else(|| {
                        vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnsupportedClause,
                            hir.origin.span(),
                            "LATERAL relation requires a preceding FROM input",
                        )]
                    })?;
                    relations.push(HirRelation {
                        id: self.relation_id(),
                        origin: hir.origin.clone(),
                        alias: None,
                        kind: HirRelationKind::Join {
                            kind: dibs_query_ir::JoinKind::Cross,
                            left: Box::new(left),
                            right: Box::new(hir),
                            predicate: None,
                            lateral: true,
                        },
                    });
                } else {
                    relations.push(hir);
                }
            }
        }

        let (distinct, targets) = match core.body {
            ast::SelectBody::Distinct(select) => match select.value {
                ast::DistinctSelectKind::On(select) => (
                    SelectDistinct::On(
                        select
                            .expressions
                            .iter()
                            .map(|expression| self.resolve_expression(expression, &scope))
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    select.targets,
                ),
                ast::DistinctSelectKind::Plain(select) => {
                    (SelectDistinct::Distinct, select.targets)
                }
            },
            ast::SelectBody::Ordinary(select) => match select.value {
                ast::OrdinarySelectKind::All(select) => (SelectDistinct::AllRows, select.targets),
                ast::OrdinarySelectKind::Unqualified(select) => {
                    (SelectDistinct::AllRows, select.targets)
                }
            },
        };
        let projections = self.resolve_projections(
            targets,
            &mut scope,
            "SELECT",
            inherited_output_names,
            synthetic_output_prefix,
        )?;

        let predicate = core
            .r#where
            .as_ref()
            .map(|clause| self.resolve_expression(&clause.expression, &scope))
            .transpose()?;
        let order_by = select
            .order_by
            .as_ref()
            .map(|clause| {
                clause
                    .items
                    .iter()
                    .map(|item| self.resolve_order_item(item, &scope))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let limit = select
            .limit
            .as_ref()
            .map(|clause| match &clause.value {
                ast::LimitValue::Expression(expression) => {
                    self.resolve_limit_expression(expression, &scope)
                }
                ast::LimitValue::AllLiteral(_) => Ok(None),
            })
            .transpose()?
            .flatten();
        let offset = select
            .offset
            .as_ref()
            .map(|clause| self.resolve_limit_expression(&clause.value, &scope))
            .transpose()?
            .flatten();

        let group_by = core
            .group_by
            .as_ref()
            .map(|clause| {
                if clause.quantifier.is_some() {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(clause.span),
                        "GROUP BY DISTINCT/ALL is outside this compiler slice",
                    )]);
                }
                clause
                    .elements
                    .iter()
                    .map(|element| match &element.value {
                        ast::GroupingElementValue::Expression(expression) => {
                            self.resolve_expression(&expression.expression, &scope)
                        }
                        _ => Err(vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnsupportedClause,
                            self.source_span(element.span),
                            "grouping sets, tuples, ROLLUP, and CUBE are outside this compiler slice",
                        )]),
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let having = core
            .having
            .as_ref()
            .map(|clause| self.resolve_expression(&clause.expression, &scope))
            .transpose()?;
        let windows = core
            .window
            .as_ref()
            .map(|clause| {
                clause
                    .definitions
                    .iter()
                    .map(|window| {
                        Ok(HirNamedWindow {
                            name: window.name.value.clone(),
                            origin: self.origin(window.span),
                            specification: self
                                .resolve_window_specification(&window.specification, &scope)?,
                        })
                    })
                    .collect::<Result<Vec<_>, DiagnosticSet>>()
            })
            .transpose()?
            .unwrap_or_default();
        let locks = select
            .locks
            .iter()
            .map(|lock| self.resolve_lock(lock, &scope))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(HirSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct,
            projections,
            from: relations,
            predicate,
            group_by,
            having,
            windows,
            order_by,
            limit,
            offset,
            locks,
        })
    }

    fn resolve_set_select(
        &mut self,
        select: ast::SelectStatement,
        parent_scope: Option<&SelectScope>,
        inherited_output_names: Option<&[String]>,
        synthetic_output_prefix: Option<&str>,
    ) -> Result<HirSelect, DiagnosticSet> {
        if select.order_by.is_some()
            || !select.locks.is_empty()
            || select.limit.is_some()
            || select.offset.is_some()
            || select.fetch.is_some()
        {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(select.span),
                "clauses attached to a set operation are outside this compiler slice",
            )]);
        }
        let ast::SelectSetNode::SetOperation(operation) = select.body.value else {
            unreachable!("set-select dispatch validated the body kind")
        };
        let ast::SetOperation {
            span,
            left,
            operator,
            quantifier,
            right,
        } = *operation;
        let scope = parent_scope.cloned().unwrap_or_default();
        let left = self.resolve_set_operand(
            left,
            &scope,
            inherited_output_names,
            synthetic_output_prefix,
        )?;
        let left_names = statement_projections(&left)
            .iter()
            .map(|projection| projection.alias.clone())
            .collect::<Vec<_>>();
        let right =
            self.resolve_set_operand(right, &scope, Some(&left_names), synthetic_output_prefix)?;
        let kind = match operator {
            ast::SetOperator::UnionOrExcept(operator)
                if compact_keyword(&operator.value) == "union" =>
            {
                SetOperationKind::Union
            }
            ast::SetOperator::UnionOrExcept(_) => SetOperationKind::Except,
            ast::SetOperator::Intersect(_) => SetOperationKind::Intersect,
        };
        let all = quantifier
            .as_ref()
            .is_some_and(|value| compact_keyword(&value.value) == "all");
        let output_names = statement_projections(&left)
            .iter()
            .map(|projection| projection.alias.clone())
            .collect::<Vec<_>>();
        let wrapper_fields = (0..output_names.len()).map(|_| self.field_id()).collect();
        Ok(self.build_set_select(left, right, kind, all, span, wrapper_fields, output_names))
    }

    fn build_set_select(
        &mut self,
        left: HirStatement,
        right: HirStatement,
        kind: SetOperationKind,
        all: bool,
        span: Span,
        wrapper_fields: Vec<FieldId>,
        output_names: Vec<String>,
    ) -> HirSelect {
        let relation_id = self.relation_id();
        let relation_origin = self.origin(span);
        let projection_inputs = statement_projections(&left)
            .iter()
            .map(|projection| {
                (
                    projection.field_id,
                    projection.alias_origin.clone(),
                    projection.expression.origin.clone(),
                )
            })
            .collect::<Vec<_>>();
        let projections = wrapper_fields
            .into_iter()
            .zip(output_names)
            .zip(projection_inputs)
            .map(
                |((field_id, alias), (source_field, alias_origin, expression_origin))| {
                    HirProjection {
                        field_id,
                        alias,
                        alias_origin,
                        expression: HirExpression {
                            id: self.expression_id(),
                            origin: expression_origin,
                            kind: HirExpressionKind::DerivedColumn {
                                binding: relation_id,
                                field_id: source_field,
                            },
                        },
                    }
                },
            )
            .collect();
        HirSelect {
            recursive: false,
            ctes: Vec::new(),
            distinct: SelectDistinct::AllRows,
            projections,
            from: vec![HirRelation {
                id: relation_id,
                origin: relation_origin,
                alias: Some(RelationAlias {
                    name: "__dibs_set".to_string(),
                    column_names: Vec::new(),
                }),
                kind: HirRelationKind::SetOperation {
                    kind,
                    all,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            }],
            predicate: None,
            group_by: Vec::new(),
            having: None,
            windows: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            locks: Vec::new(),
        }
    }

    fn resolve_set_operand(
        &mut self,
        operand: ast::SelectSetExpression,
        scope: &SelectScope,
        inherited_output_names: Option<&[String]>,
        synthetic_output_prefix: Option<&str>,
    ) -> Result<HirStatement, DiagnosticSet> {
        let span = operand.span;
        let select = ast::SelectStatement {
            span,
            body: operand,
            order_by: None,
            locks: Vec::new(),
            limit: None,
            offset: None,
            fetch: None,
        };
        Ok(HirStatement {
            id: self.statement_id(),
            origin: self.origin(span),
            kind: HirStatementKind::Select(Box::new(self.resolve_select(
                select,
                Some(scope),
                inherited_output_names,
                synthetic_output_prefix,
            )?)),
        })
    }

    fn resolve_lock(
        &self,
        lock: &ast::LockingClause,
        scope: &SelectScope,
    ) -> Result<HirLockClause, DiagnosticSet> {
        let strength = match compact_keyword(&lock.strength.value).as_str() {
            "update" => LockStrength::Update,
            "nokeyupdate" => LockStrength::NoKeyUpdate,
            "share" => LockStrength::Share,
            "keyshare" => LockStrength::KeyShare,
            _ => {
                return Err(vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::UnsupportedClause,
                    self.source_span(lock.strength.span),
                    "unknown row-lock strength",
                )]);
            }
        };
        let targets = lock
            .targets
            .iter()
            .map(|target| {
                let name = qualified_name(target);
                if target.parts.len() != 1 {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnknownRelation,
                        self.source_span(target.span),
                        "row-lock targets must name a local relation or alias",
                    )]);
                }
                scope
                    .relation(&name)
                    .map(|binding| binding.id)
                    .ok_or_else(|| {
                        vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnknownRelation,
                            self.source_span(target.span),
                            format!("unknown row-lock target '{name}'"),
                        )]
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let wait = match lock
            .wait
            .as_ref()
            .map(|wait| compact_keyword(&wait.value))
            .as_deref()
        {
            None => LockWaitPolicy::Wait,
            Some("nowait") => LockWaitPolicy::NoWait,
            Some("skiplocked") => LockWaitPolicy::SkipLocked,
            Some(_) => {
                return Err(vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::UnsupportedClause,
                    self.source_span(lock.span),
                    "unknown row-lock wait policy",
                )]);
            }
        };
        Ok(HirLockClause {
            strength,
            targets,
            wait,
        })
    }

    fn resolve_relation(
        &mut self,
        relation: Relation,
        preceding_scope: &SelectScope,
    ) -> Result<(HirRelation, Vec<RelationBinding>, bool), DiagnosticSet> {
        match relation {
            Relation::Table(table) => {
                let (hir, binding) = self.resolve_table_relation(*table, preceding_scope)?;
                Ok((hir, vec![binding], false))
            }
            Relation::Join(joined) => self.resolve_joined_relation(*joined, preceding_scope),
            Relation::Derived(derived) => {
                let lateral = derived.lateral.is_some();
                let (hir, binding) =
                    self.resolve_derived_relation(*derived, preceding_scope, lateral)?;
                Ok((hir, vec![binding], lateral))
            }
            Relation::Function(function) => {
                let lateral = function.lateral.is_some();
                let (hir, binding) = self.resolve_function_relation(*function, preceding_scope)?;
                Ok((hir, vec![binding], lateral))
            }
            relation => Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(relation_span(&relation)),
                "table functions and parenthesized relations are outside this compiler slice",
            )]),
        }
    }

    fn resolve_joined_relation(
        &mut self,
        joined: ast::JoinedRelation,
        preceding_scope: &SelectScope,
    ) -> Result<(HirRelation, Vec<RelationBinding>, bool), DiagnosticSet> {
        let (mut left, mut bindings, _) =
            self.resolve_relation_primary(joined.left, preceding_scope, false)?;
        for tail in joined.joins {
            let operator = tail
                .operator
                .value
                .chars()
                .filter(|character| !character.is_whitespace())
                .flat_map(char::to_lowercase)
                .collect::<String>();
            let kind = match operator.as_str() {
                "join" | "innerjoin" => dibs_query_ir::JoinKind::Inner,
                "leftjoin" | "leftouterjoin" => dibs_query_ir::JoinKind::Left,
                "rightjoin" | "rightouterjoin" => dibs_query_ir::JoinKind::Right,
                "fulljoin" | "fullouterjoin" => dibs_query_ir::JoinKind::Full,
                "crossjoin" => dibs_query_ir::JoinKind::Cross,
                _ => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(tail.operator.span),
                        "NATURAL JOIN is outside this compiler slice",
                    )]);
                }
            };
            let mut left_scope = SelectScope::with_parent(preceding_scope);
            left_scope
                .extend_relations(bindings.iter().cloned())
                .map_err(|diagnostic| vec![diagnostic])?;
            let right_lateral = relation_primary_is_lateral(&tail.right);
            let (right, right_bindings, _) =
                self.resolve_relation_primary(tail.right, &left_scope, right_lateral)?;
            let mut join_scope = SelectScope::with_parent(preceding_scope);
            join_scope
                .extend_relations(bindings.iter().cloned())
                .map_err(|diagnostic| vec![diagnostic])?;
            join_scope
                .extend_relations(right_bindings.iter().cloned())
                .map_err(|diagnostic| vec![diagnostic])?;
            let predicate = match (kind, tail.condition) {
                (dibs_query_ir::JoinKind::Cross, None) => None,
                (dibs_query_ir::JoinKind::Cross, Some(condition)) => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(condition.span),
                        "CROSS JOIN cannot carry an ON or USING condition",
                    )]);
                }
                (_, None) => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(tail.span),
                        "non-cross JOIN requires an ON condition",
                    )]);
                }
                (_, Some(condition)) => {
                    if condition.columns.is_some() {
                        return Err(vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnsupportedClause,
                            self.source_span(condition.span),
                            "JOIN ... USING is outside this compiler slice",
                        )]);
                    }
                    let expression = condition.expression.as_ref().ok_or_else(|| {
                        vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnsupportedClause,
                            self.source_span(condition.span),
                            "non-cross JOIN requires an ON expression",
                        )]
                    })?;
                    Some(Box::new(self.resolve_expression(expression, &join_scope)?))
                }
            };
            bindings.extend(right_bindings);
            left = HirRelation {
                id: self.relation_id(),
                origin: self.origin(tail.span),
                alias: None,
                kind: HirRelationKind::Join {
                    kind,
                    left: Box::new(left),
                    right: Box::new(right),
                    predicate,
                    lateral: right_lateral,
                },
            };
        }
        Ok((left, bindings, false))
    }

    fn resolve_relation_primary(
        &mut self,
        relation: ast::RelationPrimary,
        scope: &SelectScope,
        allow_correlation: bool,
    ) -> Result<(HirRelation, Vec<RelationBinding>, bool), DiagnosticSet> {
        match relation.value {
            ast::RelationPrimaryValue::Table(table) => {
                let (hir, binding) = self.resolve_table_relation(*table, scope)?;
                Ok((hir, vec![binding], false))
            }
            ast::RelationPrimaryValue::Derived(derived) => {
                let lateral = derived.lateral.is_some();
                let (hir, binding) =
                    self.resolve_derived_relation(*derived, scope, allow_correlation)?;
                Ok((hir, vec![binding], lateral))
            }
            ast::RelationPrimaryValue::Function(function) => {
                let lateral = function.lateral.is_some();
                let (hir, binding) = self.resolve_function_relation(*function, scope)?;
                Ok((hir, vec![binding], lateral))
            }
            _ => Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(relation.span),
                "function and parenthesized relations are outside this compiler slice",
            )]),
        }
    }

    fn resolve_function_relation(
        &mut self,
        function: ast::FunctionRelation,
        scope: &SelectScope,
    ) -> Result<(HirRelation, RelationBinding), DiagnosticSet> {
        if function.ordinality.is_some() {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(function.span),
                "WITH ORDINALITY is outside the ordinary SELECT compiler path",
            )]);
        }
        let call = &function.function;
        if call.quantifier.is_some()
            || call.star.is_some()
            || call.order_by.is_some()
            || call.filter.is_some()
            || call.within_group.is_some()
            || call
                .arguments
                .iter()
                .any(|argument| argument.name.is_some() || argument.notation.is_some())
        {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(call.span),
                "table-function modifiers are outside the ordinary SELECT compiler path",
            )]);
        }
        let authored_name = qualified_name(&call.name);
        let callable = resolve_table_callable(self.catalog, &authored_name, call.arguments.len())
            .map_err(|code| {
            vec![CompileDiagnostic::new(
                code,
                self.source_span(call.name.span),
                format!("unknown or ambiguous table function '{authored_name}'"),
            )]
        })?;
        let arguments = call
            .arguments
            .iter()
            .map(|argument| self.resolve_expression(&argument.value, scope))
            .collect::<Result<Vec<_>, _>>()?;
        let id = self.relation_id();
        let alias_names = function
            .alias
            .as_ref()
            .and_then(|alias| alias.columns.as_ref())
            .map(|columns| {
                columns
                    .columns
                    .iter()
                    .map(|column| column.value.clone())
                    .collect::<Vec<_>>()
            });
        if alias_names
            .as_ref()
            .is_some_and(|names| names.len() != callable.table_columns.len())
        {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::TypeMismatch,
                self.source_span(
                    function
                        .alias
                        .as_ref()
                        .expect("alias names require alias")
                        .span,
                ),
                "table-function column alias count must match its output arity",
            )]);
        }
        let visible_name = function.alias.as_ref().map_or_else(
            || {
                callable
                    .qualified_name
                    .rsplit('.')
                    .next()
                    .unwrap_or(&callable.qualified_name)
                    .to_string()
            },
            |alias| alias.name.value.clone(),
        );
        let output_names = alias_names.unwrap_or_else(|| {
            callable
                .table_columns
                .iter()
                .map(|column| column.name.clone())
                .collect()
        });
        let relation_alias = function.alias.as_ref().map(|alias| RelationAlias {
            name: alias.name.value.clone(),
            column_names: alias
                .columns
                .as_ref()
                .map(|_| output_names.clone())
                .unwrap_or_default(),
        });
        let columns = output_names
            .into_iter()
            .enumerate()
            .map(|(index, name)| RelationColumnBinding {
                name,
                field: RelationFieldBinding::Catalog(dibs_pg_catalog::ColumnId::new(format!(
                    "pg18:column:function:{}:{index}",
                    callable.id
                ))),
            })
            .collect();
        let origin = self.origin(function.span);
        Ok((
            HirRelation {
                id,
                origin: origin.clone(),
                alias: relation_alias,
                kind: HirRelationKind::Function {
                    callable_id: callable.id.clone(),
                    arguments,
                },
            },
            RelationBinding {
                id,
                columns,
                origin,
                visible_name,
            },
        ))
    }

    fn resolve_derived_relation(
        &mut self,
        derived: ast::DerivedRelation,
        statement_scope: &SelectScope,
        allow_correlation: bool,
    ) -> Result<(HirRelation, RelationBinding), DiagnosticSet> {
        let alias = derived.alias.ok_or_else(|| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::MissingOutputAlias,
                self.source_span(derived.span),
                "derived relations require an alias",
            )]
        })?;
        let nested_scope = if allow_correlation {
            statement_scope.clone()
        } else {
            SelectScope::cte_only(statement_scope)
        };
        let statement =
            self.resolve_nested_statement(derived.statement.value, &nested_scope, None)?;
        let projections = statement_projections(&statement);
        if let Some(columns) = &alias.columns
            && columns.columns.len() != projections.len()
        {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::TypeMismatch,
                self.source_span(columns.span),
                "derived relation column alias count must match its output arity",
            )]);
        }
        let id = self.relation_id();
        let column_names = alias
            .columns
            .as_ref()
            .map(|columns| {
                columns
                    .columns
                    .iter()
                    .map(|column| column.value.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                projections
                    .iter()
                    .map(|projection| projection.alias.clone())
                    .collect()
            });
        let relation_alias = RelationAlias {
            name: alias.name.value.clone(),
            column_names: alias
                .columns
                .as_ref()
                .map(|_| column_names.clone())
                .unwrap_or_default(),
        };
        let origin = self.origin(derived.span);
        let columns = projections
            .iter()
            .zip(column_names)
            .map(|(projection, name)| RelationColumnBinding {
                name,
                field: RelationFieldBinding::Derived(projection.field_id),
            })
            .collect();
        Ok((
            HirRelation {
                id,
                origin: origin.clone(),
                alias: Some(relation_alias),
                kind: HirRelationKind::Subquery(Box::new(statement)),
            },
            RelationBinding {
                id,
                columns,
                origin,
                visible_name: alias.name.value,
            },
        ))
    }

    fn resolve_nested_statement(
        &mut self,
        statement: Statement,
        parent_scope: &SelectScope,
        synthetic_output_prefix: Option<&str>,
    ) -> Result<HirStatement, DiagnosticSet> {
        let span = statement_span(&statement);
        let Statement::Select(select) = statement else {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(span),
                "derived relation currently requires SELECT",
            )]);
        };
        Ok(HirStatement {
            id: self.statement_id(),
            origin: self.origin(select.span),
            kind: HirStatementKind::Select(Box::new(self.resolve_select(
                *select,
                Some(parent_scope),
                None,
                synthetic_output_prefix,
            )?)),
        })
    }

    fn resolve_table_relation(
        &mut self,
        table: ast::TableRelation,
        scope: &SelectScope,
    ) -> Result<(HirRelation, RelationBinding), DiagnosticSet> {
        if table.only.is_some() {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(table.span),
                "ONLY is outside the ordinary SELECT compiler path",
            )]);
        }
        let authored_name = qualified_name(&table.name);
        let cte = (table.name.parts.len() == 1)
            .then(|| scope.cte(&authored_name))
            .flatten()
            .cloned();
        if let Some(cte) = &cte {
            self.referenced_ctes.insert(cte.id);
        }
        let resolved = if cte.is_none() {
            Some(
                resolve_table_name(self.catalog, &authored_name).map_err(|code| {
                    vec![CompileDiagnostic::new(
                        code,
                        self.source_span(table.name.span),
                        format!("unknown or ambiguous relation '{authored_name}'"),
                    )]
                })?,
            )
        } else {
            None
        };
        let id = self.relation_id();
        let alias = table.alias.as_ref().map(|alias| RelationAlias {
            name: alias.name.value.clone(),
            column_names: alias
                .columns
                .as_ref()
                .map(|columns| {
                    columns
                        .columns
                        .iter()
                        .map(|column| column.value.clone())
                        .collect()
                })
                .unwrap_or_default(),
        });
        if let Some(alias) = &table.alias
            && !alias
                .columns
                .as_ref()
                .is_none_or(|columns| columns.columns.is_empty())
        {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(alias.span),
                "table column alias lists are outside the ordinary SELECT compiler path",
            )]);
        }
        let Some(intrinsic_name) = table.name.parts.last() else {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnknownRelation,
                self.source_span(table.name.span),
                "relation name has no identifier components",
            )]);
        };
        let visible_name = alias
            .as_ref()
            .map_or_else(|| intrinsic_name.value.clone(), |alias| alias.name.clone());
        let origin = self.origin(table.span);
        let (kind, columns) = if let Some(cte) = cte {
            (HirRelationKind::Cte { cte_id: cte.id }, cte.columns)
        } else {
            let resolved = resolved.expect("catalog relation resolved when CTE is absent");
            (
                HirRelationKind::Table {
                    table_id: resolved.id.clone(),
                },
                resolved
                    .columns
                    .iter()
                    .map(|column| RelationColumnBinding {
                        name: column.name.clone(),
                        field: RelationFieldBinding::Catalog(column.id.clone()),
                    })
                    .collect(),
            )
        };
        Ok((
            HirRelation {
                id,
                origin: origin.clone(),
                alias: alias.clone(),
                kind,
            },
            RelationBinding {
                id,
                columns,
                origin,
                visible_name,
            },
        ))
    }

    fn resolve_order_item(
        &mut self,
        item: &ast::OrderByItem,
        scope: &SelectScope,
    ) -> Result<HirOrderBy, DiagnosticSet> {
        if item.using_operator.is_some() {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(item.span),
                "ORDER BY USING is outside the ordinary SELECT compiler path",
            )]);
        }
        let expression = if let Some((None, name, span)) = simple_column_name(&item.expression) {
            if let Some(projection) = scope.projection(name) {
                let mut expression = projection.expression.clone();
                expression.id = self.expression_id();
                expression.origin = self.origin(span);
                expression
            } else {
                self.resolve_expression(&item.expression, scope)?
            }
        } else {
            self.resolve_expression(&item.expression, scope)?
        };
        let direction = match item
            .direction
            .as_ref()
            .map(|direction| direction.value.as_str())
        {
            Some(value) if value.eq_ignore_ascii_case("desc") => SortDirection::Descending,
            _ => SortDirection::Ascending,
        };
        let nulls = match item.nulls.as_ref().map(|nulls| nulls.value.as_str()) {
            Some(value) if value.eq_ignore_ascii_case("first") => NullsOrder::First,
            Some(value) if value.eq_ignore_ascii_case("last") => NullsOrder::Last,
            _ => NullsOrder::Default,
        };
        Ok(HirOrderBy {
            expression,
            direction,
            nulls,
        })
    }

    fn resolve_limit_expression(
        &mut self,
        expression: &Expression,
        scope: &SelectScope,
    ) -> Result<Option<HirExpression>, DiagnosticSet> {
        let resolved = self.resolve_expression(expression, scope)?;
        match &resolved.kind {
            HirExpressionKind::Literal(HirLiteral::Integer(value)) => {
                if value.parse::<u64>().is_err() {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::InvalidLimit,
                        resolved.origin.span(),
                        "LIMIT/OFFSET must be a non-negative integer",
                    )]);
                }
            }
            HirExpressionKind::Parameter(_) => {}
            _ => {
                return Err(vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::InvalidLimit,
                    resolved.origin.span(),
                    "LIMIT/OFFSET must be a non-negative integer or declared parameter",
                )]);
            }
        }
        Ok(Some(resolved))
    }

    fn hir_operator(
        &mut self,
        span: Span,
        operator_id: impl Into<String>,
        operands: Vec<HirExpression>,
    ) -> HirExpression {
        HirExpression {
            id: self.expression_id(),
            origin: self.origin(span),
            kind: HirExpressionKind::Operator {
                operator_id: OperatorId::new(operator_id),
                operands,
            },
        }
    }

    fn resolve_expression(
        &mut self,
        expression: &Expression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            Expression::OrExpression(expression) => self.resolve_or_expression(expression, scope),
        }
    }

    fn resolve_or_expression(
        &mut self,
        expression: &OrExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            OrExpression::Or(binary) => {
                let left = self.resolve_or_expression(&binary.left, scope)?;
                let right = self.resolve_and_expression(&binary.right, scope)?;
                Ok(self.hir_operator(binary.span, "pg18:operator:syntax:OR", vec![left, right]))
            }
            OrExpression::AndExpression(expression) => {
                self.resolve_and_expression(expression, scope)
            }
        }
    }

    fn resolve_and_expression(
        &mut self,
        expression: &AndExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            AndExpression::And(binary) => {
                let left = self.resolve_and_expression(&binary.left, scope)?;
                let right = self.resolve_not_expression(&binary.right, scope)?;
                Ok(self.hir_operator(binary.span, "pg18:operator:syntax:AND", vec![left, right]))
            }
            AndExpression::NotExpression(expression) => {
                self.resolve_not_expression(expression, scope)
            }
        }
    }

    fn resolve_not_expression(
        &mut self,
        expression: &NotExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            NotExpression::Not(unary) => {
                let operand = self.resolve_not_expression(&unary.expression, scope)?;
                Ok(self.hir_operator(unary.span, "pg18:operator:syntax:NOT", vec![operand]))
            }
            NotExpression::PredicateExpression(expression) => {
                self.resolve_predicate_expression(expression, scope)
            }
        }
    }

    fn resolve_predicate_expression(
        &mut self,
        expression: &PredicateExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            PredicateExpression::ComparisonExpr(binary) => {
                let left = self.resolve_generic_expression(&binary.left, scope)?;
                let right = self.resolve_generic_expression(&binary.right, scope)?;
                Ok(self.hir_operator(
                    binary.span,
                    format!("unresolved:operator:pg_catalog.{}", binary.operator.value),
                    vec![left, right],
                ))
            }
            PredicateExpression::GenericExpression(expression) => {
                self.resolve_generic_expression(expression, scope)
            }
            PredicateExpression::IsPredicate(expression) => {
                let left = self.resolve_generic_expression(&expression.expression, scope)?;
                let negated = expression.negated.is_some();
                match &expression.test {
                    ast::IsPredicateTestNode::Value(test)
                        if compact_keyword(&test.value.value) == "null" =>
                    {
                        let operator_id = if negated {
                            dibs_query_typing::SYNTAX_IS_NOT_NULL_OPERATOR_ID
                        } else {
                            dibs_query_typing::SYNTAX_IS_NULL_OPERATOR_ID
                        };
                        Ok(self.hir_operator(expression.span, operator_id, vec![left]))
                    }
                    ast::IsPredicateTestNode::Value(test) => self.unsupported_expression(test.span),
                    ast::IsPredicateTestNode::DistinctFrom(test) => {
                        let right = self.resolve_generic_expression(&test.right, scope)?;
                        let operator_id = if negated {
                            dibs_query_typing::SYNTAX_IS_NOT_DISTINCT_FROM_OPERATOR_ID
                        } else {
                            dibs_query_typing::SYNTAX_IS_DISTINCT_FROM_OPERATOR_ID
                        };
                        Ok(self.hir_operator(expression.span, operator_id, vec![left, right]))
                    }
                }
            }
            PredicateExpression::Between(expression) => {
                self.unsupported_expression(expression.span)
            }
            PredicateExpression::In(expression) => {
                let operand = self.resolve_generic_expression(&expression.expression, scope)?;
                let values = expression
                    .values
                    .values
                    .iter()
                    .map(|value| self.resolve_expression(value, scope))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(HirExpression {
                    id: self.expression_id(),
                    origin: self.origin(expression.span),
                    kind: HirExpressionKind::InList {
                        expression: Box::new(operand),
                        values,
                        negated: expression.negated.is_some(),
                    },
                })
            }
            PredicateExpression::LikeExpr(expression) => {
                self.unsupported_expression(expression.span)
            }
            PredicateExpression::QuantifiedComparison(expression) => {
                let left = self.resolve_generic_expression(&expression.left, scope)?;
                let right = match &expression.right {
                    ast::QuantifiedRight::Expression(right) => {
                        self.resolve_expression(right, scope)?
                    }
                    ast::QuantifiedRight::StatementBody(_) => {
                        return self.unsupported_expression(expression.span);
                    }
                };
                let quantifier = match compact_keyword(&expression.quantifier.value).as_str() {
                    "any" | "some" => dibs_query_ir::ComparisonQuantifier::Any,
                    "all" => dibs_query_ir::ComparisonQuantifier::All,
                    _ => return self.unsupported_expression(expression.quantifier.span),
                };
                Ok(HirExpression {
                    id: self.expression_id(),
                    origin: self.origin(expression.span),
                    kind: HirExpressionKind::QuantifiedComparison {
                        operator_id: OperatorId::new(format!(
                            "unresolved:operator:pg_catalog.{}",
                            expression.operator.value
                        )),
                        left: Box::new(left),
                        right: Box::new(right),
                        quantifier,
                    },
                })
            }
        }
    }

    fn resolve_b_expression(
        &mut self,
        expression: &ast::BExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            ast::BExpression::GenericExpression(expression) => {
                self.resolve_generic_expression(expression, scope)
            }
        }
    }

    fn resolve_generic_expression(
        &mut self,
        expression: &GenericExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            GenericExpression::GenericExpr(binary) => {
                let left = self.resolve_generic_expression(&binary.left, scope)?;
                let right = self.resolve_additive_expression(&binary.right, scope)?;
                Ok(self.hir_operator(
                    binary.span,
                    format!("unresolved:operator:pg_catalog.{}", binary.operator.value),
                    vec![left, right],
                ))
            }
            GenericExpression::AdditiveExpression(expression) => {
                self.resolve_additive_expression(expression, scope)
            }
        }
    }

    fn resolve_additive_expression(
        &mut self,
        expression: &AdditiveExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            AdditiveExpression::AdditiveExpr(binary) => {
                let left = self.resolve_additive_expression(&binary.left, scope)?;
                let right = self.resolve_multiplicative_expression(&binary.right, scope)?;
                Ok(self.hir_operator(
                    binary.span,
                    format!("unresolved:operator:pg_catalog.{}", binary.operator.value),
                    vec![left, right],
                ))
            }
            AdditiveExpression::MultiplicativeExpression(expression) => {
                self.resolve_multiplicative_expression(expression, scope)
            }
        }
    }

    fn resolve_multiplicative_expression(
        &mut self,
        expression: &MultiplicativeExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            MultiplicativeExpression::MultiplicativeExpr(binary) => {
                let left = self.resolve_multiplicative_expression(&binary.left, scope)?;
                let right = self.resolve_exponent_expression(&binary.right, scope)?;
                Ok(self.hir_operator(
                    binary.span,
                    format!("unresolved:operator:pg_catalog.{}", binary.operator.value),
                    vec![left, right],
                ))
            }
            MultiplicativeExpression::ExponentExpression(expression) => {
                self.resolve_exponent_expression(expression, scope)
            }
        }
    }

    fn resolve_exponent_expression(
        &mut self,
        expression: &ExponentExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            ExponentExpression::ExponentExpr(binary) => {
                let left = self.resolve_exponent_expression(&binary.left, scope)?;
                let right = self.resolve_unary_expression(&binary.right, scope)?;
                Ok(self.hir_operator(
                    binary.span,
                    "unresolved:operator:pg_catalog.^",
                    vec![left, right],
                ))
            }
            ExponentExpression::UnaryExpression(expression) => {
                self.resolve_unary_expression(expression, scope)
            }
        }
    }

    fn resolve_unary_expression(
        &mut self,
        expression: &UnaryExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            UnaryExpression::Unary(unary) => {
                let operand = self.resolve_unary_expression(&unary.expression, scope)?;
                Ok(self.hir_operator(
                    unary.span,
                    format!("unresolved:operator:pg_catalog.{}", unary.operator.value),
                    vec![operand],
                ))
            }
            UnaryExpression::PostfixExpression(expression) => {
                self.resolve_postfix_expression(expression, scope)
            }
        }
    }

    fn resolve_postfix_expression(
        &mut self,
        expression: &PostfixExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        match expression {
            PostfixExpression::AtomExpression(atom) => self.resolve_atom(atom, scope),
            PostfixExpression::Window(expression) => {
                if !expression.operations.is_empty() {
                    return self.unsupported_expression(expression.span);
                }
                let mut resolved = self.resolve_call(&expression.expression, scope)?;
                let HirExpressionKind::Call(call) = &mut resolved.kind else {
                    unreachable!("callable window expression always lowers from a call")
                };
                call.over = Some(self.resolve_window_reference(&expression.window.window, scope)?);
                resolved.origin = self.origin(expression.span);
                Ok(resolved)
            }
            PostfixExpression::Postfix(expression) => {
                let mut resolved = self.resolve_atom(&expression.base, scope)?;
                for operation in &expression.operations {
                    match &operation.value {
                        ast::ValuePostfixOperationNode::CastSuffix(cast) => {
                            resolved =
                                self.resolve_cast(expression.span, resolved, &cast.type_name)?;
                        }
                        _ => return self.unsupported_expression(operation.span),
                    }
                }
                Ok(resolved)
            }
        }
    }

    fn resolve_window_reference(
        &mut self,
        window: &ast::WindowReference,
        scope: &SelectScope,
    ) -> Result<WindowReference<HirExpression>, DiagnosticSet> {
        match window {
            ast::WindowReference::DeclarationIdentifier(name) => {
                Ok(WindowReference::Named(name.value.clone()))
            }
            ast::WindowReference::WindowSpecification(specification) => Ok(
                WindowReference::Inline(self.resolve_window_specification(specification, scope)?),
            ),
        }
    }

    fn resolve_window_specification(
        &mut self,
        specification: &ast::WindowSpecification,
        scope: &SelectScope,
    ) -> Result<WindowSpec<HirExpression>, DiagnosticSet> {
        let partition_by = specification
            .partition
            .as_ref()
            .map(|partition| {
                partition
                    .expressions
                    .iter()
                    .map(|expression| self.resolve_expression(expression, scope))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let order_by = specification
            .order_by
            .as_ref()
            .map(|order| {
                order
                    .items
                    .iter()
                    .map(|item| self.resolve_order_item(item, scope))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let frame = specification
            .frame
            .as_ref()
            .map(|frame| self.resolve_window_frame(frame, scope))
            .transpose()?;
        Ok(WindowSpec {
            existing: specification.base.as_ref().map(|name| name.value.clone()),
            partition_by,
            order_by,
            frame,
        })
    }

    fn resolve_window_frame(
        &mut self,
        frame: &ast::WindowFrameClause,
        scope: &SelectScope,
    ) -> Result<WindowFrame<HirExpression>, DiagnosticSet> {
        let mode = match compact_keyword(&frame.mode.value).as_str() {
            "rows" => WindowFrameMode::Rows,
            "range" => WindowFrameMode::Range,
            "groups" => WindowFrameMode::Groups,
            _ => return self.unsupported_expression(frame.mode.span),
        };
        let exclusion = match frame
            .exclusion
            .as_ref()
            .map(|value| compact_keyword(&value.kind.value))
        {
            None => WindowExclusion::None,
            Some(value) if value == "currentrow" => WindowExclusion::CurrentRow,
            Some(value) if value == "group" => WindowExclusion::Group,
            Some(value) if value == "ties" => WindowExclusion::Ties,
            Some(_) => return self.unsupported_expression(frame.span),
        };
        Ok(WindowFrame {
            mode,
            start: self.resolve_frame_bound(&frame.start, scope)?,
            end: frame
                .end
                .as_ref()
                .map(|bound| self.resolve_frame_bound(bound, scope))
                .transpose()?,
            exclusion,
        })
    }

    fn resolve_frame_bound(
        &mut self,
        bound: &ast::FrameBound,
        scope: &SelectScope,
    ) -> Result<FrameBound<HirExpression>, DiagnosticSet> {
        let direction = bound
            .direction
            .as_ref()
            .map(|value| compact_keyword(&value.value));
        match (bound.offset.as_ref(), direction.as_deref()) {
            (None, Some("preceding")) => Ok(FrameBound::UnboundedPreceding),
            (None, Some("following")) => Ok(FrameBound::UnboundedFollowing),
            (None, None) => Ok(FrameBound::CurrentRow),
            (Some(offset), Some("preceding")) => Ok(FrameBound::Preceding(
                self.resolve_expression(offset, scope)?,
            )),
            (Some(offset), Some("following")) => Ok(FrameBound::Following(
                self.resolve_expression(offset, scope)?,
            )),
            _ => self.unsupported_expression(bound.span),
        }
    }

    fn resolve_cast(
        &mut self,
        span: Span,
        source: HirExpression,
        target_name: &ast::PgTypeName,
    ) -> Result<HirExpression, DiagnosticSet> {
        let (target_type, target_typmod) = self.resolve_parameter_type(target_name)?;
        Ok(HirExpression {
            id: self.expression_id(),
            origin: self.origin(span),
            kind: HirExpressionKind::ExplicitCast {
                target_type,
                target_typmod,
                expression: Box::new(source),
            },
        })
    }

    fn unsupported_expression<T>(&self, span: Span) -> Result<T, DiagnosticSet> {
        Err(vec![CompileDiagnostic::new(
            CompileDiagnosticCode::UnsupportedClause,
            self.source_span(span),
            "expression form is outside the ordinary SELECT compiler path",
        )])
    }

    fn resolve_call(
        &mut self,
        call: &ast::CallExpr,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        let arguments = call
            .arguments
            .iter()
            .map(|argument| self.resolve_expression(&argument.value, scope))
            .collect::<Result<Vec<_>, _>>()?;
        let argument_names = call
            .arguments
            .iter()
            .map(|argument| argument.name.as_ref().map(|name| name.value.clone()))
            .collect();
        let order_by = call
            .order_by
            .as_ref()
            .map(|clause| {
                clause
                    .items
                    .iter()
                    .map(|item| self.resolve_order_item(item, scope))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let within_group = call
            .within_group
            .as_ref()
            .map(|clause| {
                clause
                    .order_by
                    .items
                    .iter()
                    .map(|item| self.resolve_order_item(item, scope))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let filter = call
            .filter
            .as_ref()
            .map(|filter| {
                self.resolve_expression(&filter.expression, scope)
                    .map(Box::new)
            })
            .transpose()?;
        let authored_name = qualified_name(&call.name);
        let lookup_name = if authored_name.contains('.') {
            authored_name
        } else {
            format!("pg_catalog.{authored_name}")
        };
        Ok(HirExpression {
            id: self.expression_id(),
            origin: self.origin(call.span),
            kind: HirExpressionKind::Call(Box::new(HirCall {
                callable_id: CallableId::new(format!("unresolved:function:{lookup_name}")),
                arguments,
                argument_names,
                distinct: call
                    .quantifier
                    .as_ref()
                    .is_some_and(|value| value.value.eq_ignore_ascii_case("distinct")),
                star: call.star.is_some(),
                order_by,
                filter,
                within_group,
                over: None,
            })),
        })
    }
    fn resolve_atom(
        &mut self,
        atom: &AtomExpression,
        scope: &SelectScope,
    ) -> Result<HirExpression, DiagnosticSet> {
        let (span, kind) = match atom {
            AtomExpression::Call(call) => return self.resolve_call(call, scope),
            AtomExpression::NamedBind(bind) => {
                let name = bind.value.strip_prefix(':').unwrap_or(&bind.value);
                let Some(parameter) = self.parameters.get(name) else {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnknownParameter,
                        self.source_span(bind.span),
                        format!("unknown parameter ':{name}'"),
                    )]);
                };
                self.used_parameters.insert(parameter.id);
                (bind.span, HirExpressionKind::Parameter(parameter.id))
            }
            AtomExpression::Name(name) => {
                let parts = &name.name.parts;
                let (qualifier, field) = match parts.as_slice() {
                    [field] => (None, field),
                    [qualifier, field] => (Some(qualifier.value.as_str()), field),
                    _ => {
                        return Err(vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::UnsupportedClause,
                            self.source_span(name.span),
                            "qualified field names beyond relation.field are unsupported",
                        )]);
                    }
                };
                let (binding, column) = scope
                    .resolve_column(self.source_id, qualifier, &field.value, name.span)
                    .map_err(|diagnostic| vec![diagnostic])?;
                let kind = match &column.field {
                    RelationFieldBinding::Catalog(column_id) => HirExpressionKind::Column {
                        binding: binding.id,
                        column_id: column_id.clone(),
                    },
                    RelationFieldBinding::Derived(field_id) => HirExpressionKind::DerivedColumn {
                        binding: binding.id,
                        field_id: *field_id,
                    },
                    RelationFieldBinding::Cte { cte_id, field_id } => {
                        HirExpressionKind::CteColumn {
                            cte_id: *cte_id,
                            binding: binding.id,
                            field_id: *field_id,
                        }
                    }
                };
                (name.span, kind)
            }
            AtomExpression::NumericLiteral(literal) => {
                let normalized = literal.value.replace('_', "");
                let literal = if normalized.contains('.')
                    || normalized.contains('e')
                    || normalized.contains('E')
                {
                    HirLiteral::Numeric(normalized)
                } else {
                    HirLiteral::Integer(normalized)
                };
                (literal_span(atom), HirExpressionKind::Literal(literal))
            }
            AtomExpression::BooleanLiteral(literal) => (
                literal.span,
                HirExpressionKind::Literal(HirLiteral::Boolean(
                    literal.value.eq_ignore_ascii_case("true"),
                )),
            ),
            AtomExpression::NullLiteral(literal) => {
                (literal.span, HirExpressionKind::Literal(HirLiteral::Null))
            }
            AtomExpression::StringLiteral(literal) => (
                literal.span,
                HirExpressionKind::Literal(HirLiteral::String(decode_standard_string(
                    &literal.value,
                ))),
            ),
            AtomExpression::Interval(literal) => {
                let value = match &literal.value {
                    ast::IntervalValue::StringLiteral(value) => {
                        decode_standard_string(&value.value)
                    }
                    ast::IntervalValue::EscapedStringLiteral(_) => {
                        return self.unsupported_expression(literal.span);
                    }
                };
                (
                    literal.span,
                    HirExpressionKind::Literal(HirLiteral::Interval {
                        value,
                        field: literal
                            .field
                            .as_ref()
                            .map(|value| value.value.to_ascii_uppercase()),
                        to_field: literal
                            .to_field
                            .as_ref()
                            .map(|value| value.value.to_ascii_uppercase()),
                        precision: literal.precision.as_ref().map(|value| value.value.clone()),
                    }),
                )
            }
            AtomExpression::Parenthesized(parenthesized) => match &parenthesized.value {
                ParenthesizedValue::Scalar(scalar) => {
                    return self.resolve_expression(&scalar.expression, scope);
                }
                ParenthesizedValue::Subquery(subquery) => {
                    let statement = self.resolve_nested_statement(
                        subquery.statement.value.clone(),
                        scope,
                        Some("__dibs_scalar"),
                    )?;
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(parenthesized.span),
                        kind: HirExpressionKind::ScalarSubquery(Box::new(statement)),
                    });
                }
                ParenthesizedValue::RowValue(_) => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(parenthesized.span),
                        "row parentheses are outside the ordinary SELECT compiler path",
                    )]);
                }
            },
            AtomExpression::SpecialFormExpression(value) => match value {
                ast::SpecialFormExpression::Cast(cast) => {
                    let source = self.resolve_expression(&cast.expression, scope)?;
                    return self.resolve_cast(cast.span, source, &cast.type_name);
                }
                ast::SpecialFormExpression::Case(case) => {
                    let operand = case
                        .operand
                        .as_ref()
                        .map(|operand| self.resolve_expression(operand, scope).map(Box::new))
                        .transpose()?;
                    let branches = case
                        .branchs
                        .iter()
                        .map(|branch| {
                            Ok(dibs_query_ir::HirCaseBranch {
                                when: self.resolve_expression(&branch.when, scope)?,
                                then: self.resolve_expression(&branch.then, scope)?,
                            })
                        })
                        .collect::<Result<Vec<_>, DiagnosticSet>>()?;
                    let else_expression = case
                        .else_expression
                        .as_ref()
                        .map(|value| self.resolve_expression(value, scope).map(Box::new))
                        .transpose()?;
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(case.span),
                        kind: HirExpressionKind::Case {
                            operand,
                            branches,
                            else_expression,
                        },
                    });
                }
                ast::SpecialFormExpression::Coalesce(coalesce) => {
                    let arguments = coalesce
                        .arguments
                        .iter()
                        .map(|argument| self.resolve_expression(argument, scope))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(coalesce.span),
                        kind: HirExpressionKind::Coalesce(arguments),
                    });
                }
                ast::SpecialFormExpression::NullIf(nullif) => {
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(nullif.span),
                        kind: HirExpressionKind::NullIf {
                            left: Box::new(self.resolve_expression(&nullif.left, scope)?),
                            right: Box::new(self.resolve_expression(&nullif.right, scope)?),
                        },
                    });
                }
                ast::SpecialFormExpression::Greatest(greatest) => {
                    let arguments = greatest
                        .arguments
                        .iter()
                        .map(|argument| self.resolve_expression(argument, scope))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(greatest.span),
                        kind: HirExpressionKind::Greatest(arguments),
                    });
                }
                ast::SpecialFormExpression::Least(least) => {
                    let arguments = least
                        .arguments
                        .iter()
                        .map(|argument| self.resolve_expression(argument, scope))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(least.span),
                        kind: HirExpressionKind::Least(arguments),
                    });
                }
                ast::SpecialFormExpression::Extract(extract) => {
                    let field = match compact_keyword(&extract.field.value).as_str() {
                        "century" => ExtractField::Century,
                        "day" => ExtractField::Day,
                        "decade" => ExtractField::Decade,
                        "dow" => ExtractField::Dow,
                        "doy" => ExtractField::Doy,
                        "epoch" => ExtractField::Epoch,
                        "hour" => ExtractField::Hour,
                        "isodow" => ExtractField::IsoDow,
                        "isoyear" => ExtractField::IsoYear,
                        "julian" => ExtractField::Julian,
                        "microseconds" => ExtractField::Microseconds,
                        "millennium" => ExtractField::Millennium,
                        "milliseconds" => ExtractField::Milliseconds,
                        "minute" => ExtractField::Minute,
                        "month" => ExtractField::Month,
                        "quarter" => ExtractField::Quarter,
                        "second" => ExtractField::Second,
                        "timezone" => ExtractField::Timezone,
                        "timezone_hour" => ExtractField::TimezoneHour,
                        "timezone_minute" => ExtractField::TimezoneMinute,
                        "week" => ExtractField::Week,
                        "year" => ExtractField::Year,
                        _ => return self.unsupported_expression(extract.field.span),
                    };
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(extract.span),
                        kind: HirExpressionKind::Extract {
                            field,
                            source: Box::new(self.resolve_expression(&extract.source, scope)?),
                        },
                    });
                }
                ast::SpecialFormExpression::Position(position) => {
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(position.span),
                        kind: HirExpressionKind::Position {
                            substring: Box::new(
                                self.resolve_b_expression(&position.substring, scope)?,
                            ),
                            string: Box::new(self.resolve_b_expression(&position.string, scope)?),
                        },
                    });
                }
                ast::SpecialFormExpression::Exists(exists) => {
                    let statement = self.resolve_nested_statement(
                        exists.statement.value.clone(),
                        scope,
                        Some("__dibs_exists"),
                    )?;
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(exists.span),
                        kind: HirExpressionKind::Exists(Box::new(statement)),
                    });
                }
                ast::SpecialFormExpression::Array(array) => {
                    if array.statement.is_some() {
                        return self.unsupported_expression(array.span);
                    }
                    let elements = array
                        .elements
                        .iter()
                        .map(|element| self.resolve_expression(element, scope))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(HirExpression {
                        id: self.expression_id(),
                        origin: self.origin(array.span),
                        kind: HirExpressionKind::Array(elements),
                    });
                }
                _ => return self.unsupported_expression(atom_span(atom)),
            },
            _ => return self.unsupported_expression(atom_span(atom)),
        };
        Ok(HirExpression {
            id: self.expression_id(),
            origin: self.origin(span),
            kind,
        })
    }

    fn statement_id(&mut self) -> StatementId {
        let id = StatementId::new(self.next_statement);
        self.next_statement += 1;
        id
    }

    fn cte_id(&mut self) -> CteId {
        let id = CteId::new(self.next_cte);
        self.next_cte += 1;
        id
    }

    fn relation_id(&mut self) -> RelationId {
        let id = RelationId::new(self.next_relation);
        self.next_relation += 1;
        id
    }

    fn expression_id(&mut self) -> ExpressionId {
        let id = ExpressionId::new(self.next_expression);
        self.next_expression += 1;
        id
    }

    fn field_id(&mut self) -> FieldId {
        let id = FieldId::new(self.next_field);
        self.next_field += 1;
        id
    }

    fn assignment_id(&mut self) -> AssignmentId {
        let id = AssignmentId::new(self.next_assignment);
        self.next_assignment += 1;
        id
    }

    fn origin(&self, span: Span) -> SourceOrigin {
        SourceOrigin::authored(self.source_span(span))
    }

    fn source_span(&self, span: Span) -> SourceSpan {
        SourceSpan::new(self.source_id, span)
    }
}

fn resolve_table_name<'catalog>(
    catalog: &'catalog CatalogSnapshot,
    authored_name: &str,
) -> Result<&'catalog dibs_pg_catalog::CatalogTable, CompileDiagnosticCode> {
    if authored_name.contains('.') {
        return catalog
            .resolve_table(authored_name)
            .map_err(|_| CompileDiagnosticCode::UnknownRelation);
    }
    let public_name = format!("public.{authored_name}");
    if let Ok(table) = catalog.resolve_table(&public_name) {
        return Ok(table);
    }
    let suffix = format!(".{authored_name}");
    let matches = catalog
        .tables
        .iter()
        .filter(|table| table.qualified_name.ends_with(&suffix))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [table] => Ok(*table),
        [] => Err(CompileDiagnosticCode::UnknownRelation),
        _ => Err(CompileDiagnosticCode::AmbiguousRelation),
    }
}

fn resolve_table_callable<'catalog>(
    catalog: &'catalog CatalogSnapshot,
    authored_name: &str,
    arity: usize,
) -> Result<&'catalog dibs_pg_catalog::CatalogCallable, CompileDiagnosticCode> {
    let candidates = |requested: &str| {
        catalog
            .callables
            .iter()
            .filter(|callable| {
                callable.kind == CallableKind::Table
                    && callable.arguments.len() == arity
                    && if requested.contains('.') {
                        callable.qualified_name == requested
                    } else {
                        callable
                            .qualified_name
                            .rsplit('.')
                            .next()
                            .is_some_and(|name| name == requested)
                    }
            })
            .collect::<Vec<_>>()
    };

    if authored_name.contains('.') {
        return match candidates(authored_name).as_slice() {
            [callable] => Ok(*callable),
            [] => Err(CompileDiagnosticCode::UnknownCallable),
            _ => Err(CompileDiagnosticCode::AmbiguousCallable),
        };
    }

    let public_name = format!("public.{authored_name}");
    match candidates(&public_name).as_slice() {
        [callable] => return Ok(*callable),
        [] => {}
        _ => return Err(CompileDiagnosticCode::AmbiguousCallable),
    }

    match candidates(authored_name).as_slice() {
        [callable] => Ok(*callable),
        [] => Err(CompileDiagnosticCode::UnknownCallable),
        _ => Err(CompileDiagnosticCode::AmbiguousCallable),
    }
}

fn compact_keyword(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn canonical_builtin_type_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "bool" | "boolean" => "boolean".to_string(),
        "int2" | "smallint" => "smallint".to_string(),
        "int4" | "integer" | "int" => "integer".to_string(),
        "int8" | "bigint" => "bigint".to_string(),
        "float4" | "real" => "real".to_string(),
        "float8" => "double precision".to_string(),
        "decimal" | "numeric" => "numeric".to_string(),
        other => other.to_string(),
    }
}
fn cte_output_names(
    columns: Option<&ast::ColumnNameList>,
    projections: &[HirProjection],
) -> Result<Vec<String>, &'static str> {
    if let Some(columns) = columns {
        if columns.columns.len() != projections.len() {
            return Err("CTE column alias count must match its output arity");
        }
        Ok(columns
            .columns
            .iter()
            .map(|column| column.value.clone())
            .collect())
    } else {
        Ok(projections
            .iter()
            .map(|projection| projection.alias.clone())
            .collect())
    }
}

fn statement_projections(statement: &HirStatement) -> &[HirProjection] {
    match &statement.kind {
        HirStatementKind::Select(select) => &select.projections,
        HirStatementKind::Insert(insert) => &insert.returning,
        HirStatementKind::Update(update) => &update.returning,
        HirStatementKind::Delete(delete) => &delete.returning,
    }
}

fn qualified_name(name: &ast::QualifiedName) -> String {
    name.parts
        .iter()
        .map(|part| part.value.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn simple_column_name(expression: &Expression) -> Option<(Option<&str>, &str, Span)> {
    let Expression::OrExpression(OrExpression::AndExpression(AndExpression::NotExpression(
        NotExpression::PredicateExpression(PredicateExpression::GenericExpression(
            GenericExpression::AdditiveExpression(AdditiveExpression::MultiplicativeExpression(
                MultiplicativeExpression::ExponentExpression(ExponentExpression::UnaryExpression(
                    UnaryExpression::PostfixExpression(PostfixExpression::AtomExpression(
                        AtomExpression::Name(name),
                    )),
                )),
            )),
        )),
    ))) = expression
    else {
        return None;
    };
    match name.name.parts.as_slice() {
        [field] => Some((None, &field.value, field.span)),
        [qualifier, field] => Some((Some(&qualifier.value), &field.value, field.span)),
        _ => None,
    }
}

fn decode_standard_string(value: &str) -> String {
    value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .unwrap_or(value)
        .replace("''", "'")
}

fn statement_span(statement: &Statement) -> Span {
    match statement {
        Statement::With(value) => value.span,
        Statement::Select(value) => value.span,
        Statement::Values(value) => value.span,
        Statement::Insert(value) => value.span,
        Statement::Update(value) => value.span,
        Statement::Delete(value) => value.span,
    }
}

fn relation_span(relation: &Relation) -> Span {
    match relation {
        Relation::Join(value) => value.span,
        Relation::Table(value) => value.span,
        Relation::Derived(value) => value.span,
        Relation::Function(value) => value.span,
        Relation::Parenthesized(value) => value.span,
    }
}

fn relation_primary_is_lateral(relation: &ast::RelationPrimary) -> bool {
    match &relation.value {
        ast::RelationPrimaryValue::Derived(derived) => derived.lateral.is_some(),
        ast::RelationPrimaryValue::Function(function) => function.lateral.is_some(),
        _ => false,
    }
}

fn literal_span(atom: &AtomExpression) -> Span {
    atom_span(atom)
}

fn atom_span(atom: &AtomExpression) -> Span {
    match atom {
        AtomExpression::SpecialFormExpression(value) => match value {
            ast::SpecialFormExpression::Cast(value) => value.span,
            ast::SpecialFormExpression::Case(value) => value.span,
            ast::SpecialFormExpression::Coalesce(value) => value.span,
            ast::SpecialFormExpression::NullIf(value) => value.span,
            ast::SpecialFormExpression::Greatest(value) => value.span,
            ast::SpecialFormExpression::Least(value) => value.span,
            ast::SpecialFormExpression::Extract(value) => value.span,
            ast::SpecialFormExpression::Position(value) => value.span,
            ast::SpecialFormExpression::Substring(value) => value.span,
            ast::SpecialFormExpression::Overlay(value) => value.span,
            ast::SpecialFormExpression::Trim(value) => value.span,
            ast::SpecialFormExpression::Exists(value) => value.span,
            ast::SpecialFormExpression::Array(value) => value.span,
            ast::SpecialFormExpression::CurrentValue(value) => value.span,
        },
        AtomExpression::Call(value) => value.span,
        AtomExpression::Row(value) => value.span,
        AtomExpression::Parenthesized(value) => value.span,
        AtomExpression::Name(value) => value.span,
        AtomExpression::NamedBind(value)
        | AtomExpression::EscapedStringLiteral(value)
        | AtomExpression::UnicodeStringLiteral(value)
        | AtomExpression::BitStringLiteral(value)
        | AtomExpression::HexStringLiteral(value)
        | AtomExpression::StringLiteral(value)
        | AtomExpression::DollarQuotedLiteral(value)
        | AtomExpression::NumericLiteral(value)
        | AtomExpression::BooleanLiteral(value)
        | AtomExpression::NullLiteral(value) => value.span,
        AtomExpression::Interval(value) => value.span,
    }
}
