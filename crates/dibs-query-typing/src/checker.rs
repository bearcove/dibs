use crate::expression::{
    catalog_volatility, expression_has_scalar_aggregate, expression_is_group_legal,
    expression_same_value, max_volatility,
};

use super::*;

impl SemanticChecker<'_> {
    pub(super) fn check_select(
        &self,
        statement: &HirStatement,
        select: &HirSelect,
        context: &mut CheckContext<'_>,
    ) -> Result<TypedStatement, CheckError> {
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
        if let SelectDistinct::On(expressions) = &distinct {
            let mut unmatched = expressions.iter().collect::<Vec<_>>();
            for order in &order_by {
                let Some(index) = unmatched
                    .iter()
                    .position(|distinct| expression_same_value(distinct, &order.expression))
                else {
                    break;
                };
                unmatched.remove(index);
                if unmatched.is_empty() {
                    break;
                }
            }
            if !unmatched.is_empty() {
                return Err(CheckError::DistinctOnOrderMismatch {
                    origin: unmatched[0].origin.clone(),
                });
            }
        }
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
        let aggregate_query = !aggregate_projections.is_empty()
            || having.as_ref().is_some_and(|expression| {
                expression_has_scalar_aggregate(expression, self.catalog)
            });
        if aggregate_query || !group_by.is_empty() {
            if let Some(projection) = projections.iter().find(|projection| {
                !expression_is_group_legal(&projection.expression, &group_by, self.catalog)
            }) {
                return Err(CheckError::UngroupedAggregateProjection {
                    origin: projection.expression.origin.clone(),
                });
            }
            if let Some(having) = having.as_ref()
                && !expression_is_group_legal(having, &group_by, self.catalog)
            {
                return Err(CheckError::UngroupedAggregateProjection {
                    origin: having.origin.clone(),
                });
            }
        }
        let scalar_aggregate = if group_by.is_empty() && !aggregate_projections.is_empty() {
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
        if scalar_aggregate.is_none() {
            for relation in &mut typed_relations {
                self.refine_select_relation_cardinality(relation, predicate.as_ref());
            }
            cardinality = from_cardinality(&typed_relations);
            if let Some(group_cardinality) = self.predicate_bounded_group_cardinality(
                &typed_relations,
                predicate.as_ref(),
                &group_by,
            ) {
                cardinality = group_cardinality;
            }
        }
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

    pub(super) fn check_insert(
        &self,
        statement: &HirStatement,
        insert: &HirInsert,
        context: &mut CheckContext<'_>,
    ) -> Result<TypedStatement, CheckError> {
        let typed_ctes = self.check_ctes(&insert.ctes, context)?;
        let table = self.table_by_id(&insert.target)?;
        let target_types = insert
            .columns
            .iter()
            .map(|column_id| {
                table
                    .columns
                    .iter()
                    .find(|column| &column.id == column_id)
                    .map(|column| column.type_id.clone())
                    .ok_or_else(|| TypeResolutionError::MissingCatalogFact {
                        kind: "insert-target-column",
                        identity: column_id.to_string(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let (source, source_cardinality) = match &insert.source {
            HirInsertSource::Values(values) => {
                let typed = self.check_values(values.rows(), context, Some(&target_types))?;
                let rows = typed.rows().len() as u64;
                let upper = if rows == 1 {
                    UpperBound::One
                } else {
                    UpperBound::Finite(rows)
                };
                (
                    TypedInsertSource::Values(typed),
                    Cardinality::try_new(
                        LowerBound::One,
                        upper,
                        vec![CardinalityEvidence::ValuesRowCount { rows }],
                    )
                    .expect("typed VALUES are non-empty"),
                )
            }
            HirInsertSource::Select(source) => {
                let mut typed = self.check_statement(source, context)?;
                if statement_projections(&typed).len() != target_types.len() {
                    return Err(CheckError::SetColumnCountMismatch {
                        left: target_types.len(),
                        right: statement_projections(&typed).len(),
                    });
                }
                self.apply_assignment_projection_coercions(&mut typed, &target_types)?;
                let cardinality = typed.cardinality.clone();
                (TypedInsertSource::Select(Box::new(typed)), cardinality)
            }
            HirInsertSource::DefaultValues => {
                (TypedInsertSource::DefaultValues, Cardinality::exactly_one())
            }
        };

        context.bind_table(insert.target_binding, table);
        let mut action_context = context.clone();
        let conflict = insert
            .conflict
            .as_ref()
            .map(|conflict| {
                action_context.bind_table(conflict.excluded_binding, table);
                self.check_conflict(conflict, table, context, &action_context)
            })
            .transpose()?;
        let returning = self.check_projections(&insert.returning, context)?;
        let cardinality = if returning.is_empty() {
            Cardinality::empty()
        } else {
            mutation_returning_cardinality(&source_cardinality, conflict.as_ref())
        };
        Ok(TypedStatement {
            id: statement.id,
            origin: statement.origin.clone(),
            cardinality,
            kind: TypedStatementKind::Insert(Box::new(TypedInsert {
                ctes: typed_ctes,
                target: insert.target.clone(),
                target_binding: insert.target_binding,
                columns: insert.columns.clone(),
                source,
                conflict,
                returning,
            })),
        })
    }

    pub(super) fn check_update(
        &self,
        statement: &HirStatement,
        update: &HirUpdate,
        context: &mut CheckContext<'_>,
    ) -> Result<TypedStatement, CheckError> {
        let typed_ctes = self.check_ctes(&update.ctes, context)?;
        let table = self.table_by_id(&update.target)?;
        context.bind_table(update.target_binding, table);

        let mut from = Vec::with_capacity(update.from.len());
        for relation in &update.from {
            let typed = self.check_relation(relation, context)?;
            self.bind_relation(&typed, context)?;
            from.push(typed);
        }
        let assignments = update
            .assignments
            .iter()
            .map(|assignment| {
                let column = table
                    .columns
                    .iter()
                    .find(|column| column.id == assignment.target)
                    .ok_or_else(|| TypeResolutionError::MissingCatalogFact {
                        kind: "assignment-target-column",
                        identity: assignment.target.to_string(),
                    })?;
                let value =
                    self.check_expression(&assignment.value, context, Some(&column.type_id))?;
                let coercion =
                    self.coercion(&value, &column.type_id, CoercionContext::Assignment)?;
                Ok(TypedAssignment {
                    id: assignment.id,
                    target: assignment.target.clone(),
                    value,
                    coercion,
                })
            })
            .collect::<Result<Vec<_>, CheckError>>()?;
        let predicate = update
            .predicate
            .as_ref()
            .map(|predicate| self.check_predicate("UPDATE WHERE", predicate, context))
            .transpose()?;
        let returning = self.check_projections(&update.returning, context)?;
        let affected = predicate
            .as_ref()
            .and_then(|predicate| {
                self.mutation_unique_predicate_cardinality(update.target_binding, table, predicate)
                    .or_else(|| {
                        self.update_unique_cte_cardinality(update, table, &from, predicate, context)
                    })
            })
            .unwrap_or_else(Cardinality::many);
        let cardinality = if returning.is_empty() {
            Cardinality::empty()
        } else {
            Cardinality::try_new(
                LowerBound::Zero,
                affected.upper(),
                affected
                    .proof()
                    .iter()
                    .cloned()
                    .chain([CardinalityEvidence::MutationReturning])
                    .collect(),
            )
            .expect("UPDATE RETURNING preserves a valid affected-row upper bound")
        };
        Ok(TypedStatement {
            id: statement.id,
            origin: statement.origin.clone(),
            cardinality,
            kind: TypedStatementKind::Update(Box::new(TypedUpdate {
                ctes: typed_ctes,
                target: update.target.clone(),
                target_binding: update.target_binding,
                assignments,
                from,
                predicate,
                returning,
            })),
        })
    }

    pub(super) fn check_delete(
        &self,
        statement: &HirStatement,
        delete: &HirDelete,
        context: &mut CheckContext<'_>,
    ) -> Result<TypedStatement, CheckError> {
        let typed_ctes = self.check_ctes(&delete.ctes, context)?;
        let table = self.table_by_id(&delete.target)?;
        context.bind_table(delete.target_binding, table);
        let mut using_relations = Vec::with_capacity(delete.using_relations.len());
        for relation in &delete.using_relations {
            let typed = self.check_relation(relation, context)?;
            self.bind_relation(&typed, context)?;
            using_relations.push(typed);
        }
        let predicate = delete
            .predicate
            .as_ref()
            .map(|predicate| self.check_predicate("DELETE WHERE", predicate, context))
            .transpose()?;
        let returning = self.check_projections(&delete.returning, context)?;
        let affected = predicate
            .as_ref()
            .and_then(|predicate| {
                self.mutation_unique_predicate_cardinality(delete.target_binding, table, predicate)
            })
            .unwrap_or_else(Cardinality::many);
        let cardinality = if returning.is_empty() {
            Cardinality::empty()
        } else {
            Cardinality::try_new(
                LowerBound::Zero,
                affected.upper(),
                affected
                    .proof()
                    .iter()
                    .cloned()
                    .chain([CardinalityEvidence::MutationReturning])
                    .collect(),
            )
            .expect("DELETE RETURNING preserves a valid affected-row upper bound")
        };
        Ok(TypedStatement {
            id: statement.id,
            origin: statement.origin.clone(),
            cardinality,
            kind: TypedStatementKind::Delete(Box::new(TypedDelete {
                ctes: typed_ctes,
                target: delete.target.clone(),
                target_binding: delete.target_binding,
                using_relations,
                predicate,
                returning,
            })),
        })
    }

    fn predicate_bounded_group_cardinality(
        &self,
        relations: &[TypedRelation],
        predicate: Option<&TypedExpression>,
        group_by: &[TypedExpression],
    ) -> Option<Cardinality> {
        let predicate = predicate?;
        let [group] = group_by else {
            return None;
        };
        let TypedExpressionKind::Column { binding, .. } = &group.kind else {
            return None;
        };
        if !relations.iter().any(|relation| {
            self.relation_binding_is_predicate_bounded(relation, *binding, predicate)
        }) {
            return None;
        }
        Some(
            Cardinality::try_new(
                LowerBound::Zero,
                UpperBound::One,
                vec![CardinalityEvidence::PredicateBoundedGroup { binding: *binding }],
            )
            .expect("predicate-bounded grouping cardinality is valid"),
        )
    }

    fn relation_binding_is_predicate_bounded(
        &self,
        relation: &TypedRelation,
        binding: RelationId,
        predicate: &TypedExpression,
    ) -> bool {
        match &relation.kind {
            TypedRelationKind::Table { table_id } if relation.id == binding => {
                self.table_by_id(table_id).ok().is_some_and(|table| {
                    self.unique_binding_cardinality(binding, table, predicate, false)
                        .is_some()
                })
            }
            TypedRelationKind::Join {
                kind: JoinKind::Inner | JoinKind::Left,
                left,
                ..
            } => self.relation_binding_is_predicate_bounded(left, binding, predicate),
            TypedRelationKind::Join {
                kind: JoinKind::Right,
                right,
                ..
            } => self.relation_binding_is_predicate_bounded(right, binding, predicate),
            TypedRelationKind::Table { .. }
            | TypedRelationKind::Join { .. }
            | TypedRelationKind::Cte { .. }
            | TypedRelationKind::Subquery(_)
            | TypedRelationKind::Function { .. }
            | TypedRelationKind::Values { .. }
            | TypedRelationKind::SetOperation { .. } => false,
        }
    }

    fn refine_select_relation_cardinality(
        &self,
        relation: &mut TypedRelation,
        select_predicate: Option<&TypedExpression>,
    ) {
        match &mut relation.kind {
            TypedRelationKind::Table { table_id } => {
                let Ok(table) = self.table_by_id(table_id) else {
                    return;
                };
                if let Some(cardinality) = select_predicate.and_then(|predicate| {
                    self.unique_binding_cardinality(relation.id, table, predicate, false)
                }) {
                    relation.cardinality = cardinality;
                }
            }
            TypedRelationKind::Join {
                kind,
                left,
                right,
                predicate,
                ..
            } => {
                self.refine_select_relation_cardinality(left, select_predicate);
                self.refine_select_relation_cardinality(right, select_predicate);
                let right_per_left_at_most_one = right.cardinality.upper() == UpperBound::One
                    || predicate.as_deref().is_some_and(|predicate| {
                        self.relation_unique_under_predicate(right, predicate)
                    });
                let left_per_right_at_most_one = left.cardinality.upper() == UpperBound::One
                    || predicate.as_deref().is_some_and(|predicate| {
                        self.relation_unique_under_predicate(left, predicate)
                    });
                let at_most_one = match kind {
                    JoinKind::Inner => {
                        (left.cardinality.upper() == UpperBound::One && right_per_left_at_most_one)
                            || (right.cardinality.upper() == UpperBound::One
                                && left_per_right_at_most_one)
                    }
                    JoinKind::Left => {
                        left.cardinality.upper() == UpperBound::One && right_per_left_at_most_one
                    }
                    JoinKind::Right => {
                        right.cardinality.upper() == UpperBound::One && left_per_right_at_most_one
                    }
                    JoinKind::Cross | JoinKind::Full => false,
                };
                if at_most_one {
                    relation.cardinality = Cardinality::try_new(
                        match kind {
                            JoinKind::Left => left.cardinality.lower(),
                            JoinKind::Right => right.cardinality.lower(),
                            JoinKind::Inner | JoinKind::Cross | JoinKind::Full => LowerBound::Zero,
                        },
                        UpperBound::One,
                        vec![CardinalityEvidence::Join {
                            relation: relation.id,
                        }],
                    )
                    .expect("unique-key join cardinality is valid");
                }
            }
            TypedRelationKind::Cte { .. }
            | TypedRelationKind::Subquery(_)
            | TypedRelationKind::Function { .. }
            | TypedRelationKind::Values { .. }
            | TypedRelationKind::SetOperation { .. } => {}
        }
    }

    fn relation_unique_under_predicate(
        &self,
        relation: &TypedRelation,
        predicate: &TypedExpression,
    ) -> bool {
        match &relation.kind {
            TypedRelationKind::Table { table_id } => self
                .catalog
                .tables
                .iter()
                .find(|table| &table.id == table_id)
                .is_some_and(|table| {
                    self.unique_binding_cardinality(relation.id, table, predicate, true)
                        .is_some()
                }),
            TypedRelationKind::Join {
                kind: JoinKind::Inner,
                left,
                right,
                predicate: Some(join_predicate),
                ..
            } => {
                let left_bounded = self.relation_unique_under_predicate(left, predicate);
                let right_bounded = self.relation_unique_under_predicate(right, predicate);
                let right_per_left = right.cardinality.upper() == UpperBound::One
                    || self.relation_unique_under_predicate(right, join_predicate);
                let left_per_right = left.cardinality.upper() == UpperBound::One
                    || self.relation_unique_under_predicate(left, join_predicate);
                (left_bounded && right_per_left) || (right_bounded && left_per_right)
            }
            TypedRelationKind::Join { .. }
            | TypedRelationKind::Cte { .. }
            | TypedRelationKind::Subquery(_)
            | TypedRelationKind::Function { .. }
            | TypedRelationKind::Values { .. }
            | TypedRelationKind::SetOperation { .. } => false,
        }
    }

    fn unique_binding_cardinality(
        &self,
        binding: RelationId,
        table: &CatalogTable,
        predicate: &TypedExpression,
        allow_other_binding: bool,
    ) -> Option<Cardinality> {
        std::iter::once((&table.primary_key.id, table.primary_key.columns.as_slice()))
            .chain(
                table
                    .unique_constraints
                    .iter()
                    .map(|constraint| (&constraint.id, constraint.columns.as_slice())),
            )
            .find_map(|(constraint_id, constraint_columns)| {
                let columns = constraint_columns
                    .iter()
                    .map(|name| table.column(name).map(|column| column.id.clone()))
                    .collect::<Option<Vec<_>>>()?;
                (!columns.is_empty()
                    && columns.iter().all(|column| {
                        self.predicate_constrains_binding_column(
                            binding,
                            predicate,
                            column,
                            allow_other_binding,
                        )
                    }))
                .then(|| {
                    Cardinality::at_most_one_with(CardinalityEvidence::UniquePredicate {
                        constraint_id: constraint_id.clone(),
                        columns,
                    })
                })
            })
    }

    fn predicate_constrains_binding_column(
        &self,
        binding: RelationId,
        predicate: &TypedExpression,
        column: &ColumnId,
        allow_other_binding: bool,
    ) -> bool {
        let TypedExpressionKind::Operator {
            authored_operator_id,
            operator_id,
            operands,
        } = &predicate.kind
        else {
            return false;
        };
        if authored_operator_id.as_str() == SYNTAX_AND_OPERATOR_ID {
            return operands.iter().any(|operand| {
                self.predicate_constrains_binding_column(
                    binding,
                    &operand.expression,
                    column,
                    allow_other_binding,
                )
            });
        }
        let is_equality = self
            .catalog
            .operator_candidates("pg_catalog.=", 2)
            .any(|operator| &operator.id == operator_id);
        if !is_equality || operands.len() != 2 {
            return false;
        }
        let matches = |target: &TypedExpression, value: &TypedExpression| {
            matches!(
                &target.kind,
                TypedExpressionKind::Column {
                    binding: target_binding,
                    column_id,
                } if *target_binding == binding && column_id == column
            ) && match &value.kind {
                TypedExpressionKind::Literal(_) | TypedExpressionKind::Parameter(_) => true,
                TypedExpressionKind::Column {
                    binding: value_binding,
                    ..
                }
                | TypedExpressionKind::DerivedColumn {
                    binding: value_binding,
                    ..
                }
                | TypedExpressionKind::CteColumn {
                    binding: value_binding,
                    ..
                } => allow_other_binding && *value_binding != binding,
                _ => false,
            }
        };
        matches(&operands[0].expression, &operands[1].expression)
            || matches(&operands[1].expression, &operands[0].expression)
    }

    fn mutation_unique_predicate_cardinality(
        &self,
        target_binding: RelationId,
        table: &CatalogTable,
        predicate: &TypedExpression,
    ) -> Option<Cardinality> {
        std::iter::once((&table.primary_key.id, table.primary_key.columns.as_slice()))
            .chain(
                table
                    .unique_constraints
                    .iter()
                    .map(|constraint| (&constraint.id, constraint.columns.as_slice())),
            )
            .find_map(|(constraint_id, constraint_columns)| {
                if constraint_columns.is_empty() {
                    return None;
                }
                let columns = constraint_columns
                    .iter()
                    .map(|name| table.column(name).map(|column| column.id.clone()))
                    .collect::<Option<Vec<_>>>()?;
                if columns.iter().all(|column| {
                    self.predicate_constrains_target_column(target_binding, predicate, column)
                }) {
                    Some(Cardinality::at_most_one_with(
                        CardinalityEvidence::UniquePredicate {
                            constraint_id: constraint_id.clone(),
                            columns,
                        },
                    ))
                } else {
                    None
                }
            })
    }

    fn predicate_constrains_target_column(
        &self,
        target_binding: RelationId,
        predicate: &TypedExpression,
        column: &ColumnId,
    ) -> bool {
        let TypedExpressionKind::Operator {
            authored_operator_id,
            operator_id,
            operands,
        } = &predicate.kind
        else {
            return false;
        };
        if authored_operator_id.as_str() == SYNTAX_AND_OPERATOR_ID {
            return operands.iter().any(|operand| {
                self.predicate_constrains_target_column(target_binding, &operand.expression, column)
            });
        }
        let is_equality = self
            .catalog
            .operator_candidates("pg_catalog.=", 2)
            .any(|operator| &operator.id == operator_id);
        if !is_equality || operands.len() != 2 {
            return false;
        }
        let matches = |target: &TypedExpression, value: &TypedExpression| {
            matches!(
                &target.kind,
                TypedExpressionKind::Column { binding, column_id }
                    if *binding == target_binding && column_id == column
            ) && row_independent_scalar(value)
        };
        matches(&operands[0].expression, &operands[1].expression)
            || matches(&operands[1].expression, &operands[0].expression)
    }

    fn update_unique_cte_cardinality(
        &self,
        update: &HirUpdate,
        table: &CatalogTable,
        from: &[TypedRelation],
        predicate: &TypedExpression,
        context: &CheckContext<'_>,
    ) -> Option<Cardinality> {
        let TypedExpressionKind::Operator {
            operator_id,
            operands,
            ..
        } = &predicate.kind
        else {
            return None;
        };
        let operator = self
            .catalog
            .operator_candidates("pg_catalog.=", 2)
            .find(|operator| &operator.id == operator_id)?;
        if operator.qualified_name != "pg_catalog.=" || operands.len() != 2 {
            return None;
        }
        let match_sides = |target: &TypedExpression, source: &TypedExpression| {
            let TypedExpressionKind::Column { binding, column_id } = &target.kind else {
                return None;
            };
            if *binding != update.target_binding {
                return None;
            }
            let TypedExpressionKind::CteColumn {
                cte_id,
                binding,
                field_id,
            } = &source.kind
            else {
                return None;
            };
            let cte = context.ctes.get(cte_id)?;
            cte.fields.get(field_id)?;
            let cte_relation = from.iter().find(|relation| {
                relation.id == *binding
                    && matches!(
                        relation.kind,
                        TypedRelationKind::Cte { cte_id: candidate } if candidate == *cte_id
                    )
            })?;
            if !matches!(
                cte_relation.cardinality.upper(),
                UpperBound::Zero | UpperBound::One
            ) {
                return None;
            }
            let constraint_id = if table.primary_key.columns.len() == 1
                && table
                    .column(&table.primary_key.columns[0])
                    .is_some_and(|column| column.id == *column_id)
            {
                table.primary_key.id.clone()
            } else {
                table
                    .unique_constraints
                    .iter()
                    .find(|constraint| {
                        constraint.columns.len() == 1
                            && table
                                .column(&constraint.columns[0])
                                .is_some_and(|column| column.id == *column_id)
                    })?
                    .id
                    .clone()
            };
            Some(Cardinality::at_most_one_with(
                CardinalityEvidence::MutationUniqueCteJoin {
                    constraint_id,
                    columns: vec![column_id.clone()],
                    cte: *cte_id,
                },
            ))
        };
        match_sides(&operands[0].expression, &operands[1].expression)
            .or_else(|| match_sides(&operands[1].expression, &operands[0].expression))
    }

    fn check_conflict(
        &self,
        conflict: &HirConflictClause,
        table: &CatalogTable,
        target_context: &CheckContext<'_>,
        action_context: &CheckContext<'_>,
    ) -> Result<TypedConflictClause, CheckError> {
        let target = match &conflict.target {
            HirConflictTarget::Constraint(constraint) => {
                ConflictTarget::Constraint(constraint.clone())
            }
            HirConflictTarget::Inference {
                expressions,
                predicate,
            } => ConflictTarget::Inference {
                expressions: expressions
                    .iter()
                    .map(|expression| self.check_expression(expression, target_context, None))
                    .collect::<Result<Vec<_>, _>>()?,
                predicate: predicate
                    .as_deref()
                    .map(|predicate| {
                        self.check_predicate("ON CONFLICT WHERE", predicate, target_context)
                    })
                    .transpose()?
                    .map(Box::new),
            },
            HirConflictTarget::Unspecified => ConflictTarget::Unspecified,
        };
        let action = match &conflict.action {
            HirConflictAction::Nothing => TypedConflictAction::Nothing,
            HirConflictAction::Update {
                assignments,
                predicate,
            } => TypedConflictAction::Update {
                assignments: assignments
                    .iter()
                    .map(|assignment| {
                        let column = table
                            .columns
                            .iter()
                            .find(|column| column.id == assignment.target)
                            .ok_or_else(|| TypeResolutionError::MissingCatalogFact {
                                kind: "assignment-target-column",
                                identity: assignment.target.to_string(),
                            })?;
                        let value = self.check_expression(
                            &assignment.value,
                            action_context,
                            Some(&column.type_id),
                        )?;
                        let coercion =
                            self.coercion(&value, &column.type_id, CoercionContext::Assignment)?;
                        Ok(TypedAssignment {
                            id: assignment.id,
                            target: assignment.target.clone(),
                            value,
                            coercion,
                        })
                    })
                    .collect::<Result<Vec<_>, CheckError>>()?,
                predicate: predicate
                    .as_ref()
                    .map(|predicate| {
                        self.check_predicate(
                            "ON CONFLICT DO UPDATE WHERE",
                            predicate,
                            action_context,
                        )
                    })
                    .transpose()?
                    .map(Box::new),
            },
        };
        Ok(TypedConflictClause {
            target,
            excluded_binding: conflict.excluded_binding,
            action,
        })
    }

    fn check_ctes(
        &self,
        ctes: &[HirCte],
        context: &mut CheckContext<'_>,
    ) -> Result<Vec<TypedCte>, CheckError> {
        let mut typed = Vec::with_capacity(ctes.len());
        for cte in ctes {
            let statement = if cte.recursive {
                let wrapper_projections = hir_statement_projections(&cte.statement);
                let HirStatementKind::Select(wrapper) = &cte.statement.kind else {
                    return Err(CheckError::UnsupportedRecursiveCte {
                        origin: cte.origin.clone(),
                    });
                };
                let [set_relation] = wrapper.from.as_slice() else {
                    return Err(CheckError::UnsupportedRecursiveCte {
                        origin: cte.origin.clone(),
                    });
                };
                let HirRelationKind::SetOperation {
                    kind: SetOperationKind::Union,
                    left: anchor,
                    ..
                } = &set_relation.kind
                else {
                    return Err(CheckError::UnsupportedRecursiveCte {
                        origin: cte.origin.clone(),
                    });
                };
                let anchor = self.check_statement(anchor, context)?;
                let anchor_projections = statement_projections(&anchor);
                if wrapper_projections.len() != anchor_projections.len() {
                    return Err(CheckError::SetColumnCountMismatch {
                        left: wrapper_projections.len(),
                        right: anchor_projections.len(),
                    });
                }
                context.ctes.insert(
                    cte.id,
                    CheckedCte {
                        fields: wrapper_projections
                            .iter()
                            .zip(anchor_projections)
                            .map(|(wrapper, anchor)| {
                                (wrapper.field_id, projection_output_expression(anchor))
                            })
                            .collect(),
                        cardinality: Cardinality::unknown(),
                    },
                );
                self.check_statement(&cte.statement, context)?
            } else {
                self.check_statement(&cte.statement, context)?
            };
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
                CheckedCte {
                    fields: statement_projections(&statement)
                        .iter()
                        .map(|projection| {
                            (
                                projection.field_id,
                                projection_output_expression(projection),
                            )
                        })
                        .collect(),
                    cardinality: statement.cardinality.clone(),
                },
            );
            typed.push(TypedCte::try_new(
                cte.id,
                cte.recursive,
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
                let cte = context.ctes.get(cte_id).ok_or_else(|| {
                    TypeResolutionError::MissingCatalogFact {
                        kind: "cte",
                        identity: cte_id.to_string(),
                    }
                })?;
                let cardinality = Cardinality::try_new(
                    cte.cardinality.lower(),
                    cte.cardinality.upper(),
                    vec![CardinalityEvidence::CtePropagation { cte: *cte_id }],
                )
                .expect("CTE propagation preserves valid bounds");
                (cardinality, TypedRelationKind::Cte { cte_id: *cte_id })
            }
            HirRelationKind::Subquery(statement) => {
                let mut nested = context.clone();
                let statement = self.check_statement(statement, &mut nested)?;
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
                let cardinality = match callable.cardinality {
                    CallableCardinality::ExactlyOne => Cardinality::try_new(
                        LowerBound::One,
                        UpperBound::One,
                        vec![CardinalityEvidence::RegisteredFunction {
                            callable_id: callable.id.clone(),
                        }],
                    )
                    .expect("registered exactly-one function cardinality is valid"),
                    CallableCardinality::ZeroOrOne => Cardinality::try_new(
                        LowerBound::Zero,
                        UpperBound::One,
                        vec![CardinalityEvidence::RegisteredFunction {
                            callable_id: callable.id.clone(),
                        }],
                    )
                    .expect("registered zero-or-one function cardinality is valid"),
                    CallableCardinality::OnePerInput => Cardinality::unknown(),
                    CallableCardinality::SetOfUnknown => Cardinality::try_new(
                        LowerBound::Zero,
                        UpperBound::Unbounded,
                        vec![CardinalityEvidence::RegisteredFunction {
                            callable_id: callable.id.clone(),
                        }],
                    )
                    .expect("registered set-returning function cardinality is valid"),
                };
                (
                    cardinality,
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
                let cte = context.ctes.get(cte_id).ok_or_else(|| {
                    TypeResolutionError::MissingCatalogFact {
                        kind: "cte",
                        identity: cte_id.to_string(),
                    }
                })?;
                context.cte_bindings.insert(relation.id, *cte_id);
                context.relations.insert(
                    relation.id,
                    cte.fields
                        .iter()
                        .map(|(field_id, expression)| {
                            (
                                RelationField::Derived(*field_id),
                                BoundColumn {
                                    type_id: expression.type_id.clone(),
                                    typmod: expression.typmod.clone(),
                                    nullable: expression.nullability.is_nullable(),
                                    volatility: expression.volatility,
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
                                RelationField::Catalog(ColumnId::new(format!(
                                    "pg18:column:function:{}:{index}",
                                    callable.id
                                ))),
                                BoundColumn {
                                    type_id: column.type_id.clone(),
                                    typmod: None,
                                    nullable: column.nullability == CatalogNullability::Nullable,
                                    volatility: catalog_volatility(callable.volatility),
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
                                RelationField::Catalog(ColumnId::new(format!(
                                    "pg18:column:values:{}:{index}",
                                    relation.id
                                ))),
                                BoundColumn {
                                    type_id: column.type_id.clone(),
                                    typmod: column.typmod.clone(),
                                    nullable: column.nullability.is_nullable(),
                                    volatility: max_volatility(
                                        rows.rows()
                                            .iter()
                                            .map(|row| row[index].expression.volatility),
                                    ),
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

fn row_independent_scalar(expression: &TypedExpression) -> bool {
    match &expression.kind {
        TypedExpressionKind::Literal(_) | TypedExpressionKind::Parameter(_) => true,
        TypedExpressionKind::Cast { expression, .. }
        | TypedExpressionKind::ExplicitCast { expression, .. }
        | TypedExpressionKind::Collate { expression, .. } => row_independent_scalar(expression),
        _ => false,
    }
}

fn mutation_returning_cardinality(
    source: &Cardinality,
    conflict: Option<&TypedConflictClause>,
) -> Cardinality {
    let lower = match conflict.map(|conflict| &conflict.action) {
        Some(TypedConflictAction::Nothing)
        | Some(TypedConflictAction::Update {
            predicate: Some(_), ..
        }) => LowerBound::Zero,
        _ => source.lower(),
    };
    Cardinality::try_new(
        lower,
        source.upper(),
        vec![CardinalityEvidence::MutationReturning],
    )
    .expect("mutation RETURNING cannot exceed its source cardinality")
}
