use crate::expression::{expression_has_scalar_aggregate, expression_is_aggregate_legal};

use super::*;

impl SemanticChecker<'_> {
    pub(super) fn check_select(
        &self,
        statement: &HirStatement,
        select: &HirSelect,
        context: &mut CheckContext<'_>,
    ) -> Result<TypedStatement, CheckError> {
        if select.recursive {
            return Err(CheckError::UnsupportedRecursiveCte {
                origin: statement.origin.clone(),
            });
        }
        let typed_ctes = self.check_ctes(&select.ctes, context)?;
        let mut typed_relations = Vec::with_capacity(select.from.len());
        for relation in &select.from {
            let typed = self.check_relation(relation, context)?;
            self.bind_relation(&typed, context)?;
            typed_relations.push(typed);
        }
        let projections = self.check_projections(&select.projections, context)?;
        let distinct = self.check_distinct(&select.distinct, context)?;
        let predicate = select
            .predicate
            .as_ref()
            .map(|expression| self.check_predicate("WHERE", expression, context))
            .transpose()?;
        let group_by = select
            .group_by
            .iter()
            .map(|expression| self.check_expression(expression, context, None))
            .collect::<Result<Vec<_>, _>>()?;
        let having = select
            .having
            .as_ref()
            .map(|expression| self.check_predicate("HAVING", expression, context))
            .transpose()?;
        let windows = select
            .windows
            .iter()
            .map(|window| self.check_named_window(window, context))
            .collect::<Result<Vec<_>, _>>()?;
        let order_by = select
            .order_by
            .iter()
            .map(|order| self.check_order_by(order, context))
            .collect::<Result<Vec<_>, _>>()?;
        let (limit, constant_limit) = select
            .limit
            .as_ref()
            .map(|expression| self.check_limit("LIMIT", expression, context))
            .transpose()?
            .map_or((None, None), |(typed, value)| (Some(typed), value));
        let (offset, _) = select
            .offset
            .as_ref()
            .map(|expression| self.check_limit("OFFSET", expression, context))
            .transpose()?
            .map_or((None, None), |(typed, value)| (Some(typed), value));

        let aggregate_projections = projections
            .iter()
            .filter(|projection| {
                expression_has_scalar_aggregate(&projection.expression, self.catalog)
            })
            .collect::<Vec<_>>();
        let scalar_aggregate = if group_by.is_empty() && !aggregate_projections.is_empty() {
            if let Some(projection) = projections.iter().find(|projection| {
                !expression_is_aggregate_legal(&projection.expression, self.catalog)
            }) {
                return Err(CheckError::UngroupedAggregateProjection {
                    origin: projection.expression.origin.clone(),
                });
            }
            Some(aggregate_projections[0])
        } else {
            None
        };
        let mut cardinality = if let Some(projection) = scalar_aggregate {
            Cardinality::try_new(
                if having.is_some() {
                    LowerBound::Zero
                } else {
                    LowerBound::One
                },
                UpperBound::One,
                vec![CardinalityEvidence::ScalarAggregate {
                    expression: projection.expression.id,
                }],
            )
            .expect("scalar aggregate range is valid")
        } else {
            from_cardinality(&typed_relations)
        };
        if let Some(limit) = constant_limit {
            cardinality = cardinality.limit(limit);
        }
        Ok(TypedStatement {
            id: statement.id,
            origin: statement.origin.clone(),
            cardinality,
            kind: TypedStatementKind::Select(Box::new(TypedSelect {
                recursive: select.recursive,
                ctes: typed_ctes,
                distinct,
                projections,
                from: typed_relations,
                predicate,
                group_by,
                having,
                windows,
                order_by,
                limit,
                offset,
                locks: select.locks.clone(),
            })),
        })
    }

    fn check_ctes(
        &self,
        ctes: &[HirCte],
        context: &mut CheckContext<'_>,
    ) -> Result<Vec<TypedCte>, CheckError> {
        let mut typed = Vec::with_capacity(ctes.len());
        for cte in ctes {
            let statement = self.check_statement(&cte.statement, context)?;
            let output_fields = statement_projections(&statement)
                .iter()
                .map(|projection| projection.field_id)
                .collect::<Vec<_>>();
            let output_names = statement_projections(&statement)
                .iter()
                .map(|projection| projection.sql_label.clone())
                .collect::<Vec<_>>();
            context.ctes.insert(
                cte.id,
                statement_projections(&statement)
                    .iter()
                    .map(|projection| {
                        (
                            projection.field_id,
                            projection_output_expression(projection),
                        )
                    })
                    .collect(),
            );
            typed.push(TypedCte::try_new(
                cte.id,
                cte.name.clone(),
                cte.materialization,
                Box::new(statement),
                output_fields,
                output_names,
            )?);
        }
        Ok(typed)
    }

    fn check_projections(
        &self,
        projections: &[HirProjection],
        context: &CheckContext<'_>,
    ) -> Result<Vec<TypedProjection>, CheckError> {
        projections
            .iter()
            .map(|projection| {
                Ok(TypedProjection {
                    field_id: projection.field_id,
                    sql_label: projection.alias.clone(),
                    expression: self.check_expression(&projection.expression, context, None)?,
                    coercion: None,
                })
            })
            .collect()
    }

    fn check_distinct(
        &self,
        distinct: &SelectDistinct<HirExpression>,
        context: &CheckContext<'_>,
    ) -> Result<SelectDistinct<TypedExpression>, CheckError> {
        match distinct {
            SelectDistinct::AllRows => Ok(SelectDistinct::AllRows),
            SelectDistinct::Distinct => Ok(SelectDistinct::Distinct),
            SelectDistinct::On(expressions) => Ok(SelectDistinct::On(
                expressions
                    .iter()
                    .map(|expression| self.check_expression(expression, context, None))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
        }
    }

    pub(super) fn check_predicate(
        &self,
        clause: &'static str,
        expression: &HirExpression,
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        let typed = self.check_expression(expression, context, Some(&self.types.boolean))?;
        if typed.type_id != self.types.boolean {
            return Err(CheckError::NonBooleanPredicate {
                clause,
                actual: typed.type_id,
                origin: expression.origin.clone(),
            });
        }
        Ok(typed)
    }

    fn check_limit(
        &self,
        clause: &'static str,
        expression: &HirExpression,
        context: &CheckContext<'_>,
    ) -> Result<(TypedLimit, Option<u64>), CheckError> {
        match &expression.kind {
            HirExpressionKind::Literal(HirLiteral::Integer(value)) => {
                let value = value.parse::<u64>().map_err(|_| CheckError::InvalidLimit {
                    clause,
                    origin: expression.origin.clone(),
                })?;
                Ok((TypedLimit::Constant(value), Some(value)))
            }
            HirExpressionKind::Parameter(parameter_id) => {
                let parameter = context.parameters.get(parameter_id).ok_or_else(|| {
                    CheckError::UnknownParameter {
                        parameter_id: *parameter_id,
                        origin: expression.origin.clone(),
                    }
                })?;
                if !self.is_integer(&parameter.type_id) {
                    return Err(CheckError::InvalidLimit {
                        clause,
                        origin: expression.origin.clone(),
                    });
                }
                Ok((TypedLimit::Parameter(*parameter_id), None))
            }
            _ => Err(CheckError::InvalidLimit {
                clause,
                origin: expression.origin.clone(),
            }),
        }
    }

    fn check_relation(
        &self,
        relation: &HirRelation,
        context: &mut CheckContext<'_>,
    ) -> Result<TypedRelation, CheckError> {
        let (cardinality, kind) = match &relation.kind {
            HirRelationKind::Table { table_id } => {
                self.table_by_id(table_id)?;
                (
                    Cardinality::many(),
                    TypedRelationKind::Table {
                        table_id: table_id.clone(),
                    },
                )
            }
            HirRelationKind::Cte { cte_id } => {
                if !context.ctes.contains_key(cte_id) {
                    return Err(TypeResolutionError::MissingCatalogFact {
                        kind: "cte",
                        identity: cte_id.to_string(),
                    }
                    .into());
                }
                (
                    Cardinality::many(),
                    TypedRelationKind::Cte { cte_id: *cte_id },
                )
            }
            HirRelationKind::Subquery(statement) => {
                let statement = self.check_statement(statement, context)?;
                (
                    statement.cardinality.clone(),
                    TypedRelationKind::Subquery(Box::new(statement)),
                )
            }
            HirRelationKind::Function {
                callable_id,
                arguments,
            } => {
                let direct = self.callable_by_id(callable_id)?;
                if direct.kind != CallableKind::Table {
                    return Err(TypeResolutionError::MissingCatalogFact {
                        kind: "table-callable",
                        identity: callable_id.to_string(),
                    }
                    .into());
                }
                let initial = arguments
                    .iter()
                    .map(|argument| self.check_expression(argument, context, None))
                    .collect::<Result<Vec<_>, _>>()?;
                let actual = initial
                    .iter()
                    .map(|argument| self.known_type(&argument.type_id))
                    .collect::<Vec<_>>();
                let selected = self
                    .select_pg_candidate([direct], &actual, |candidate| {
                        candidate.arguments.iter().collect::<Vec<_>>()
                    })
                    .map_err(|_| TypeResolutionError::IncompatibleCallable {
                        name: direct.qualified_name.clone(),
                        argument_types: actual.clone(),
                    })?;
                let ResolvedCandidate {
                    candidate: callable,
                    argument_types,
                } = selected;
                let arguments = arguments
                    .iter()
                    .zip(&argument_types)
                    .map(|(argument, expected)| {
                        self.check_expression(argument, context, Some(expected))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                (
                    Cardinality::unknown(),
                    TypedRelationKind::Function {
                        callable_id: callable.id.clone(),
                        arguments,
                    },
                )
            }
            HirRelationKind::Join {
                kind,
                left,
                right,
                predicate,
                lateral,
            } => {
                let left = self.check_relation(left, context)?;
                self.bind_relation(&left, context)?;
                let right = self.check_relation(right, context)?;
                self.bind_relation(&right, context)?;
                let predicate = predicate
                    .as_deref()
                    .map(|expression| self.check_predicate("JOIN ON", expression, context))
                    .transpose()?
                    .map(Box::new);
                let cardinality = join_cardinality(relation.id, *kind, &left, &right);
                (
                    cardinality,
                    TypedRelationKind::Join {
                        kind: *kind,
                        left: Box::new(left),
                        right: Box::new(right),
                        predicate,
                        lateral: *lateral,
                    },
                )
            }
            HirRelationKind::Values { rows } => {
                let rows = self.check_values(rows.rows(), context, None)?;
                let row_count = rows.rows().len() as u64;
                let cardinality = Cardinality::try_new(
                    LowerBound::One,
                    if row_count == 1 {
                        UpperBound::One
                    } else {
                        UpperBound::Finite(row_count)
                    },
                    vec![CardinalityEvidence::ValuesRowCount { rows: row_count }],
                )
                .expect("HIR VALUES are non-empty");
                (cardinality, TypedRelationKind::Values { rows })
            }
            HirRelationKind::SetOperation {
                kind,
                all,
                left,
                right,
            } => {
                let mut left = self.check_statement(left, context)?;
                let mut right = self.check_statement(right, context)?;
                self.coerce_set_outputs(&mut left, &mut right)?;
                let cardinality = set_cardinality(relation.id, *kind, *all, &left, &right);
                (
                    cardinality,
                    TypedRelationKind::SetOperation {
                        kind: *kind,
                        all: *all,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                )
            }
        };
        Ok(TypedRelation {
            id: relation.id,
            origin: relation.origin.clone(),
            alias: relation.alias.clone(),
            cardinality,
            kind,
        })
    }

    fn bind_relation(
        &self,
        relation: &TypedRelation,
        context: &mut CheckContext<'_>,
    ) -> Result<(), CheckError> {
        match &relation.kind {
            TypedRelationKind::Table { table_id } => {
                context.bind_table(relation.id, self.table_by_id(table_id)?);
            }
            TypedRelationKind::Cte { cte_id } => {
                let fields = context.ctes.get(cte_id).ok_or_else(|| {
                    TypeResolutionError::MissingCatalogFact {
                        kind: "cte",
                        identity: cte_id.to_string(),
                    }
                })?;
                context.relations.insert(
                    relation.id,
                    fields
                        .iter()
                        .map(|(field_id, expression)| {
                            (
                                synthetic_field_column(relation.id, *field_id),
                                BoundColumn {
                                    type_id: expression.type_id.clone(),
                                    typmod: expression.typmod.clone(),
                                    nullable: expression.nullability.is_nullable(),
                                },
                            )
                        })
                        .collect(),
                );
            }
            TypedRelationKind::Subquery(statement) => {
                context.bind_projection(relation.id, statement);
            }
            TypedRelationKind::Function { callable_id, .. } => {
                let callable = self.callable_by_id(callable_id)?;
                context.relations.insert(
                    relation.id,
                    callable
                        .table_columns
                        .iter()
                        .enumerate()
                        .map(|(index, column)| {
                            (
                                ColumnId::new(format!(
                                    "pg18:column:function:{}:{index}",
                                    callable.id
                                )),
                                BoundColumn {
                                    type_id: column.type_id.clone(),
                                    typmod: None,
                                    nullable: column.nullability == CatalogNullability::Nullable,
                                },
                            )
                        })
                        .collect(),
                );
            }
            TypedRelationKind::Join {
                kind, left, right, ..
            } => {
                self.bind_relation(left, context)?;
                self.bind_relation(right, context)?;
                match kind {
                    JoinKind::Left => {
                        collect_relation_ids(right, &mut context.null_extended);
                    }
                    JoinKind::Right => {
                        collect_relation_ids(left, &mut context.null_extended);
                    }
                    JoinKind::Full => {
                        collect_relation_ids(left, &mut context.null_extended);
                        collect_relation_ids(right, &mut context.null_extended);
                    }
                    JoinKind::Inner | JoinKind::Cross => {}
                }
            }
            TypedRelationKind::Values { rows } => {
                context.relations.insert(
                    relation.id,
                    rows.columns()
                        .iter()
                        .enumerate()
                        .map(|(index, column)| {
                            (
                                ColumnId::new(format!(
                                    "pg18:column:values:{}:{index}",
                                    relation.id
                                )),
                                BoundColumn {
                                    type_id: column.type_id.clone(),
                                    typmod: column.typmod.clone(),
                                    nullable: column.nullability.is_nullable(),
                                },
                            )
                        })
                        .collect(),
                );
            }
            TypedRelationKind::SetOperation { left, .. } => {
                context.bind_projection(relation.id, left);
            }
        }
        Ok(())
    }

    fn table_by_id(&self, id: &TableId) -> Result<&CatalogTable, CheckError> {
        self.catalog
            .tables
            .iter()
            .find(|table| &table.id == id)
            .ok_or_else(|| {
                TypeResolutionError::MissingCatalogFact {
                    kind: "table",
                    identity: id.to_string(),
                }
                .into()
            })
    }

    fn callable_by_id(&self, id: &CallableId) -> Result<&CatalogCallable, CheckError> {
        self.catalog.callable_by_id(id).ok_or_else(|| {
            TypeResolutionError::MissingCatalogFact {
                kind: "callable",
                identity: id.to_string(),
            }
            .into()
        })
    }
}

fn collect_relation_ids(relation: &TypedRelation, ids: &mut BTreeSet<RelationId>) {
    ids.insert(relation.id);
    if let TypedRelationKind::Join { left, right, .. } = &relation.kind {
        collect_relation_ids(left, ids);
        collect_relation_ids(right, ids);
    }
}

fn from_cardinality(relations: &[TypedRelation]) -> Cardinality {
    if relations.is_empty() {
        return Cardinality::exactly_one();
    }
    let lower = if relations
        .iter()
        .all(|relation| relation.cardinality.lower() == LowerBound::One)
    {
        LowerBound::One
    } else {
        LowerBound::Zero
    };
    let upper = relations
        .iter()
        .map(|relation| relation.cardinality.upper())
        .try_fold(1_u64, |product, upper| match upper {
            UpperBound::Zero => Some(0),
            UpperBound::One => Some(product),
            UpperBound::Finite(value) => product.checked_mul(value),
            UpperBound::Unbounded | UpperBound::Unknown => None,
        })
        .map_or(UpperBound::Unbounded, |product| match product {
            0 => UpperBound::Zero,
            1 => UpperBound::One,
            value => UpperBound::Finite(value),
        });
    Cardinality::try_new(lower, upper, vec![CardinalityEvidence::Conservative])
        .expect("FROM product range is valid")
}

fn join_cardinality(
    relation_id: RelationId,
    kind: JoinKind,
    left: &TypedRelation,
    right: &TypedRelation,
) -> Cardinality {
    let lower = match kind {
        JoinKind::Left if left.cardinality.lower() == LowerBound::One => LowerBound::One,
        JoinKind::Right if right.cardinality.lower() == LowerBound::One => LowerBound::One,
        JoinKind::Full
            if left.cardinality.lower() == LowerBound::One
                || right.cardinality.lower() == LowerBound::One =>
        {
            LowerBound::One
        }
        JoinKind::Cross | JoinKind::Inner
            if left.cardinality.lower() == LowerBound::One
                && right.cardinality.lower() == LowerBound::One =>
        {
            LowerBound::One
        }
        _ => LowerBound::Zero,
    };
    Cardinality::try_new(
        lower,
        UpperBound::Unbounded,
        vec![CardinalityEvidence::Join {
            relation: relation_id,
        }],
    )
    .expect("join range is valid")
}

fn set_cardinality(
    relation_id: RelationId,
    kind: SetOperationKind,
    _all: bool,
    left: &TypedStatement,
    right: &TypedStatement,
) -> Cardinality {
    let lower = match kind {
        SetOperationKind::Union
            if left.cardinality.lower() == LowerBound::One
                || right.cardinality.lower() == LowerBound::One =>
        {
            LowerBound::One
        }
        SetOperationKind::Intersect
            if left.cardinality.lower() == LowerBound::One
                && right.cardinality.lower() == LowerBound::One =>
        {
            LowerBound::Zero
        }
        SetOperationKind::Except => LowerBound::Zero,
        _ => LowerBound::Zero,
    };
    Cardinality::try_new(
        lower,
        UpperBound::Unbounded,
        vec![CardinalityEvidence::SetOperation {
            relation: relation_id,
        }],
    )
    .expect("set-operation range is valid")
}

fn projection_output_expression(projection: &TypedProjection) -> TypedExpression {
    let Some(coercion) = &projection.coercion else {
        return projection.expression.clone();
    };
    let mut expression = projection.expression.clone();
    expression.type_id = coercion.target_type.clone();
    expression.typmod = coercion.target_typmod.clone();
    expression.nullability = coercion.result_nullability.clone();
    expression
}
