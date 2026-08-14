use std::collections::{BTreeMap, BTreeSet};

use dibs_pg_catalog::{CallableId, CatalogSnapshot, OperatorId};
use dibs_query_ir::{
    ExpressionId, FieldId, FrameBound, HirCall, HirExpression, HirExpressionKind, HirLiteral,
    HirNamedWindow, HirOrderBy, HirParameter, HirProjection, HirQuery, HirRelation,
    HirRelationKind, HirSelect, HirStatement, HirStatementKind, NullsOrder, ParameterId, QueryId,
    RelationAlias, RelationId, SelectDistinct, SortDirection, SourceOrigin, StatementId,
    WindowExclusion, WindowFrame, WindowFrameMode, WindowReference, WindowSpec,
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
    ProjectionBinding, RelationBinding, RelationColumnBinding, RelationFieldBinding, SelectScope,
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
    next_relation: u32,
    next_expression: u32,
    next_field: u32,
    parameters: BTreeMap<String, HirParameter>,
    used_parameters: BTreeSet<ParameterId>,
}

impl<'catalog> Resolver<'catalog> {
    fn new(source_id: SourceId, catalog: &'catalog CatalogSnapshot, query_id: QueryId) -> Self {
        Self {
            source_id,
            catalog,
            query_id,
            next_statement: 1,
            next_relation: 1,
            next_expression: 1,
            next_field: 1,
            parameters: BTreeMap::new(),
            used_parameters: BTreeSet::new(),
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
        let span = statement_span(&statement);
        let Statement::Select(select) = statement else {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(span),
                "ordinary SELECT compiler path only accepts SELECT statements",
            )]);
        };
        let id = self.statement_id();
        let origin = self.origin(select.span);
        let kind = HirStatementKind::Select(Box::new(self.resolve_select(*select, None)?));
        Ok(HirStatement { id, origin, kind })
    }

    fn resolve_select(
        &mut self,
        select: ast::SelectStatement,
        parent_scope: Option<&SelectScope>,
    ) -> Result<HirSelect, DiagnosticSet> {
        if !select.locks.is_empty() || select.fetch.is_some() {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(select.span),
                "row locks and FETCH are outside the ordinary SELECT compiler path",
            )]);
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
        let mut projections = Vec::new();
        for target in targets {
            match target.value {
                ast::SelectTargetValue::Expression(target) => {
                    let expression = self.resolve_expression(&target.expression, &scope)?;
                    let (alias, alias_span) = if let Some(alias) = target.alias {
                        (alias.name.value, alias.name.span)
                    } else if let Some((_, name, span)) = simple_column_name(&target.expression) {
                        (name.to_string(), span)
                    } else {
                        return Err(vec![CompileDiagnostic::new(
                            CompileDiagnosticCode::MissingOutputAlias,
                            self.source_span(target.span),
                            "computed SELECT expressions require an explicit alias",
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
                        "wildcard projections are not accepted by the ordinary SELECT compiler path",
                    )]);
                }
                ast::SelectTargetValue::QualifiedWildcard(target) => {
                    return Err(vec![CompileDiagnostic::new(
                        CompileDiagnosticCode::UnsupportedClause,
                        self.source_span(target.span),
                        "wildcard projections are not accepted by the ordinary SELECT compiler path",
                    )]);
                }
            }
        }

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
            locks: Vec::new(),
        })
    }

    fn resolve_relation(
        &mut self,
        relation: Relation,
        preceding_scope: &SelectScope,
    ) -> Result<(HirRelation, Vec<RelationBinding>, bool), DiagnosticSet> {
        match relation {
            Relation::Table(table) => {
                let (hir, binding) = self.resolve_table_relation(*table)?;
                Ok((hir, vec![binding], false))
            }
            Relation::Join(joined) => self.resolve_joined_relation(*joined, preceding_scope),
            Relation::Derived(derived) => {
                let lateral = derived.lateral.is_some();
                let correlation_scope = lateral.then_some(preceding_scope);
                let (hir, binding) = self.resolve_derived_relation(*derived, correlation_scope)?;
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
        let (mut left, mut bindings, _) = self.resolve_relation_primary(joined.left, None)?;
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
                self.resolve_relation_primary(tail.right, right_lateral.then_some(&left_scope))?;
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
        correlation_scope: Option<&SelectScope>,
    ) -> Result<(HirRelation, Vec<RelationBinding>, bool), DiagnosticSet> {
        match relation.value {
            ast::RelationPrimaryValue::Table(table) => {
                let (hir, binding) = self.resolve_table_relation(*table)?;
                Ok((hir, vec![binding], false))
            }
            ast::RelationPrimaryValue::Derived(derived) => {
                let lateral = derived.lateral.is_some();
                let (hir, binding) = self.resolve_derived_relation(*derived, correlation_scope)?;
                Ok((hir, vec![binding], lateral))
            }
            _ => Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(relation.span),
                "function and parenthesized relations are outside this compiler slice",
            )]),
        }
    }

    fn resolve_derived_relation(
        &mut self,
        derived: ast::DerivedRelation,
        correlation_scope: Option<&SelectScope>,
    ) -> Result<(HirRelation, RelationBinding), DiagnosticSet> {
        let alias = derived.alias.ok_or_else(|| {
            vec![CompileDiagnostic::new(
                CompileDiagnosticCode::MissingOutputAlias,
                self.source_span(derived.span),
                "derived relations require an alias",
            )]
        })?;
        let statement =
            self.resolve_nested_statement(derived.statement.value, correlation_scope)?;
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
        parent_scope: Option<&SelectScope>,
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
            kind: HirStatementKind::Select(Box::new(self.resolve_select(*select, parent_scope)?)),
        })
    }

    fn resolve_table_relation(
        &mut self,
        table: ast::TableRelation,
    ) -> Result<(HirRelation, RelationBinding), DiagnosticSet> {
        if table.only.is_some() {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(table.span),
                "ONLY is outside the ordinary SELECT compiler path",
            )]);
        }
        let authored_name = qualified_name(&table.name);
        let resolved = resolve_table_name(self.catalog, &authored_name).map_err(|code| {
            vec![CompileDiagnostic::new(
                code,
                self.source_span(table.name.span),
                format!("unknown or ambiguous relation '{authored_name}'"),
            )]
        })?;
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
        Ok((
            HirRelation {
                id,
                origin: origin.clone(),
                alias: alias.clone(),
                kind: HirRelationKind::Table {
                    table_id: resolved.id.clone(),
                },
            },
            RelationBinding {
                id,
                columns: resolved
                    .columns
                    .iter()
                    .map(|column| RelationColumnBinding {
                        name: column.name.clone(),
                        field: RelationFieldBinding::Catalog(column.id.clone()),
                    })
                    .collect(),
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
                self.unsupported_expression(expression.span)
            }
            PredicateExpression::Between(expression) => {
                self.unsupported_expression(expression.span)
            }
            PredicateExpression::In(expression) => self.unsupported_expression(expression.span),
            PredicateExpression::LikeExpr(expression) => {
                self.unsupported_expression(expression.span)
            }
            PredicateExpression::QuantifiedComparison(expression) => {
                self.unsupported_expression(expression.span)
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
            PostfixExpression::Postfix(expression) => self.unsupported_expression(expression.span),
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
        if call
            .arguments
            .iter()
            .any(|argument| argument.name.is_some() || argument.notation.is_some())
        {
            return Err(vec![CompileDiagnostic::new(
                CompileDiagnosticCode::UnsupportedClause,
                self.source_span(call.span),
                "named function arguments are outside the ordinary SELECT compiler path",
            )]);
        }
        let arguments = call
            .arguments
            .iter()
            .map(|argument| self.resolve_expression(&argument.value, scope))
            .collect::<Result<Vec<_>, _>>()?;
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
            AtomExpression::Parenthesized(parenthesized) => match &parenthesized.value {
                ParenthesizedValue::Scalar(scalar) => {
                    return self.resolve_expression(&scalar.expression, scope);
                }
                ParenthesizedValue::Subquery(subquery) => {
                    let statement = self
                        .resolve_nested_statement(subquery.statement.value.clone(), Some(scope))?;
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
            _ => {
                return Err(vec![CompileDiagnostic::new(
                    CompileDiagnosticCode::UnsupportedClause,
                    self.source_span(atom_span(atom)),
                    "expression atom is outside the ordinary SELECT compiler path",
                )]);
            }
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
