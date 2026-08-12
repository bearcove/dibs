use super::*;

impl SemanticChecker<'_> {
    pub(super) fn check_expression(
        &self,
        expression: &HirExpression,
        context: &CheckContext<'_>,
        expected: Option<&TypeId>,
    ) -> Result<TypedExpression, CheckError> {
        match &expression.kind {
            HirExpressionKind::Literal(literal) => {
                self.check_literal(expression, literal, expected)
            }
            HirExpressionKind::Parameter(parameter_id) => {
                let parameter = context.parameters.get(parameter_id).ok_or_else(|| {
                    CheckError::UnknownParameter {
                        parameter_id: *parameter_id,
                        origin: expression.origin.clone(),
                    }
                })?;
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: parameter.type_id.clone(),
                    typmod: parameter.typmod.clone(),
                    nullability: if parameter.nullable {
                        Nullability::nullable(NullabilityEvidence::Conservative)
                    } else {
                        synthetic_not_null("parameter")
                    },
                    volatility: Volatility::Immutable,
                    kind: TypedExpressionKind::Parameter(*parameter_id),
                })
            }
            HirExpressionKind::Column { binding, column_id } => {
                let column = context
                    .relations
                    .get(binding)
                    .and_then(|columns| columns.get(&RelationField::Catalog(column_id.clone())))
                    .ok_or_else(|| CheckError::UnknownColumn {
                        binding: *binding,
                        column_id: column_id.clone(),
                        origin: expression.origin.clone(),
                    })?;
                let null_extended = context.null_extended.contains(binding);
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: column.type_id.clone(),
                    typmod: column.typmod.clone(),
                    nullability: if null_extended || column.nullable {
                        Nullability::nullable(if null_extended {
                            NullabilityEvidence::OuterJoinNullExtension { binding: *binding }
                        } else {
                            NullabilityEvidence::BaseColumnNullable {
                                column_id: column_id.clone(),
                            }
                        })
                    } else {
                        Nullability::not_null(NullabilityEvidence::BaseColumnNotNull {
                            column_id: column_id.clone(),
                        })
                    },
                    volatility: Volatility::Immutable,
                    kind: TypedExpressionKind::Column {
                        binding: *binding,
                        column_id: column_id.clone(),
                    },
                })
            }
            HirExpressionKind::DerivedColumn { binding, field_id } => {
                let column = context
                    .relations
                    .get(binding)
                    .and_then(|columns| columns.get(&RelationField::Derived(*field_id)))
                    .ok_or_else(|| CheckError::UnknownColumn {
                        binding: *binding,
                        column_id: synthetic_field_column(*binding, *field_id),
                        origin: expression.origin.clone(),
                    })?;
                let null_extended = context.null_extended.contains(binding);
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: column.type_id.clone(),
                    typmod: column.typmod.clone(),
                    nullability: if null_extended || column.nullable {
                        Nullability::nullable(if null_extended {
                            NullabilityEvidence::OuterJoinNullExtension { binding: *binding }
                        } else {
                            NullabilityEvidence::Conservative
                        })
                    } else {
                        synthetic_not_null("derived-output")
                    },
                    volatility: column.volatility,
                    kind: TypedExpressionKind::DerivedColumn {
                        binding: *binding,
                        field_id: *field_id,
                    },
                })
            }
            HirExpressionKind::Call(call) => self.check_call(expression, call, context),
            HirExpressionKind::Operator {
                operator_id,
                operands,
            } => self.check_operator(expression, operator_id, operands, context),
            HirExpressionKind::Cast {
                cast_id,
                expression: source,
            } => self.check_explicit_cast(expression, cast_id, source, context),
            HirExpressionKind::Collate {
                collation_id,
                expression: source,
            } => {
                if self.catalog.collation_by_id(collation_id).is_none() {
                    return Err(TypeResolutionError::MissingCatalogFact {
                        kind: "collation",
                        identity: collation_id.to_string(),
                    }
                    .into());
                }
                let source = self.check_expression(source, context, expected)?;
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: source.type_id.clone(),
                    typmod: source.typmod.clone(),
                    nullability: source.nullability.clone(),
                    volatility: source.volatility,
                    kind: TypedExpressionKind::Collate {
                        collation_id: collation_id.clone(),
                        expression: Box::new(source),
                    },
                })
            }
            HirExpressionKind::Case {
                operand,
                branches,
                else_expression,
            } => self.check_case(
                expression,
                operand.as_deref(),
                branches,
                else_expression.as_deref(),
                context,
                expected,
            ),
            HirExpressionKind::ScalarSubquery(statement) => {
                let mut nested = context.clone();
                let statement = self.check_statement(statement, &mut nested)?;
                let projections = statement_projections(&statement);
                if projections.len() != 1 {
                    return Err(CheckError::SetColumnCountMismatch {
                        left: 1,
                        right: projections.len(),
                    });
                }
                if !matches!(
                    statement.cardinality.upper(),
                    UpperBound::Zero | UpperBound::One | UpperBound::Finite(0 | 1)
                ) {
                    return Err(CheckError::UnboundedScalarSubquery {
                        origin: expression.origin.clone(),
                        cardinality: statement.cardinality.clone(),
                    });
                }
                let projection = &projections[0];
                let nullability = if statement.cardinality.lower() == LowerBound::Zero {
                    Nullability::nullable(NullabilityEvidence::ScalarSubqueryZeroRows {
                        relation: RelationId::new(statement.id.get()),
                    })
                } else {
                    projection.output_nullability().clone()
                };
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: projection.output_type_id().clone(),
                    typmod: projection.output_typmod().cloned(),
                    nullability,
                    volatility: projection.expression.volatility,
                    kind: TypedExpressionKind::ScalarSubquery(Box::new(statement)),
                })
            }
            HirExpressionKind::Row(values) => {
                let values = values
                    .iter()
                    .map(|value| self.check_expression(value, context, None))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: synthetic_row_type(&values),
                    typmod: None,
                    nullability: synthetic_not_null("row-constructor"),
                    volatility: max_volatility(values.iter().map(|value| value.volatility)),
                    kind: TypedExpressionKind::Row(values),
                })
            }
            HirExpressionKind::Array(elements) => {
                self.check_array(expression, elements, context, expected)
            }
            HirExpressionKind::CteColumn { cte_id, field_id } => {
                let value = context
                    .ctes
                    .get(cte_id)
                    .and_then(|fields| fields.get(field_id))
                    .ok_or_else(|| CheckError::UnknownCteField {
                        cte_id: *cte_id,
                        field_id: *field_id,
                        origin: expression.origin.clone(),
                    })?;
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: value.type_id.clone(),
                    typmod: value.typmod.clone(),
                    nullability: if value.nullability.is_nullable() {
                        Nullability::nullable(NullabilityEvidence::CtePropagation { cte: *cte_id })
                    } else {
                        Nullability::not_null(NullabilityEvidence::CtePropagation { cte: *cte_id })
                    },
                    volatility: value.volatility,
                    kind: TypedExpressionKind::CteColumn {
                        cte_id: *cte_id,
                        field_id: *field_id,
                    },
                })
            }
        }
    }

    fn check_literal(
        &self,
        expression: &HirExpression,
        literal: &HirLiteral,
        expected: Option<&TypeId>,
    ) -> Result<TypedExpression, CheckError> {
        let (type_id, nullability) = match literal {
            HirLiteral::Null => (
                expected
                    .cloned()
                    .unwrap_or_else(|| self.types.unknown.clone()),
                Nullability::nullable(NullabilityEvidence::NullLiteral),
            ),
            HirLiteral::Boolean(_) => (
                self.types.boolean.clone(),
                synthetic_not_null("boolean-literal"),
            ),
            HirLiteral::Integer(value) => {
                self.validate_numeric_literal(
                    value,
                    &self.types.integer,
                    true,
                    &expression.origin,
                )?;
                (
                    self.types.integer.clone(),
                    synthetic_not_null("integer-literal"),
                )
            }
            HirLiteral::Numeric(value) => {
                self.validate_numeric_literal(
                    value,
                    &self.types.numeric,
                    false,
                    &expression.origin,
                )?;
                (
                    self.types.numeric.clone(),
                    synthetic_not_null("numeric-literal"),
                )
            }
            HirLiteral::String(_) => (
                expected
                    .cloned()
                    .unwrap_or_else(|| self.types.unknown.clone()),
                synthetic_not_null("string-literal"),
            ),
            HirLiteral::Bytes(_) => (
                expected
                    .cloned()
                    .unwrap_or_else(|| self.types.bytea.clone()),
                synthetic_not_null("bytes-literal"),
            ),
        };
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id,
            typmod: None,
            nullability,
            volatility: Volatility::Immutable,
            kind: TypedExpressionKind::Literal(literal.clone()),
        })
    }

    fn validate_numeric_literal(
        &self,
        value: &str,
        target: &TypeId,
        integer_syntax: bool,
        origin: &SourceOrigin,
    ) -> Result<(), CheckError> {
        let valid = if target == &self.types.smallint {
            integer_syntax && value.parse::<i16>().is_ok()
        } else if target == &self.types.integer {
            integer_syntax && value.parse::<i32>().is_ok()
        } else if target == &self.types.bigint {
            integer_syntax && value.parse::<i64>().is_ok()
        } else if target == &self.types.numeric {
            valid_postgres_numeric_literal(value)
        } else {
            true
        };
        if valid {
            Ok(())
        } else {
            Err(CheckError::NumericLiteralOutOfRange {
                value: value.to_string(),
                target: target.clone(),
                origin: origin.clone(),
            })
        }
    }

    fn check_operator(
        &self,
        expression: &HirExpression,
        authored_id: &OperatorId,
        operands: &[HirExpression],
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        if let Some(kind) = structural_operator(authored_id) {
            return self.check_structural_operator(
                expression,
                authored_id,
                kind,
                operands,
                context,
            );
        }
        let initial = operands
            .iter()
            .map(|operand| self.check_expression(operand, context, None))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = initial
            .iter()
            .map(|operand| self.known_type(&operand.type_id))
            .collect::<Vec<_>>();
        let selected = self.select_operator(
            operator_candidates(self.catalog, authored_id, operands.len()),
            &actual,
            authored_id,
        )?;
        let ResolvedCandidate {
            candidate: operator,
            argument_types,
        } = selected;
        let declared = operator
            .left
            .iter()
            .chain(operator.right.iter())
            .cloned()
            .collect::<Vec<_>>();
        let arguments = operands
            .iter()
            .zip(&argument_types)
            .map(|(operand, expected)| {
                self.check_argument(operand, context, expected, CoercionContext::Implicit)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let nullable = arguments
            .iter()
            .any(|argument| argument.expression.nullability.is_nullable());
        let result =
            self.resolve_polymorphic_result(&operator.result, &declared, &argument_types)?;
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: result,
            typmod: None,
            nullability: callable_nullability(operator.id.as_str(), nullable),
            volatility: max_volatility(
                arguments
                    .iter()
                    .map(|argument| argument.expression.volatility),
            ),
            kind: TypedExpressionKind::Operator {
                authored_operator_id: authored_id.clone(),
                operator_id: operator.id.clone(),
                operands: arguments,
            },
        })
    }

    fn check_structural_operator(
        &self,
        expression: &HirExpression,
        authored_id: &OperatorId,
        kind: StructuralOperator,
        operands: &[HirExpression],
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        let expected_arity = match kind {
            StructuralOperator::Not
            | StructuralOperator::IsNull
            | StructuralOperator::IsNotNull => 1,
            StructuralOperator::And
            | StructuralOperator::Or
            | StructuralOperator::IsDistinctFrom
            | StructuralOperator::IsNotDistinctFrom => 2,
        };
        if operands.len() != expected_arity {
            return Err(TypeResolutionError::IncompatibleOperator {
                operator: authored_id.clone(),
                operand_types: vec![None; operands.len()],
            }
            .into());
        }
        let arguments = match kind {
            StructuralOperator::Not | StructuralOperator::And | StructuralOperator::Or => operands
                .iter()
                .map(|operand| {
                    self.check_argument(
                        operand,
                        context,
                        &self.types.boolean,
                        CoercionContext::Implicit,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            StructuralOperator::IsNull | StructuralOperator::IsNotNull => vec![TypedArgument {
                expression: self.check_expression(&operands[0], context, None)?,
                coercion: None,
            }],
            StructuralOperator::IsDistinctFrom | StructuralOperator::IsNotDistinctFrom => {
                let first = self.check_expression(&operands[0], context, None)?;
                let second = self.check_expression(&operands[1], context, None)?;
                let common = self.common_type(&[first.type_id, second.type_id])?;
                operands
                    .iter()
                    .map(|operand| {
                        self.check_argument(operand, context, &common, CoercionContext::Implicit)
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
        };
        let nullable = matches!(
            kind,
            StructuralOperator::Not | StructuralOperator::And | StructuralOperator::Or
        ) && arguments
            .iter()
            .any(|argument| argument.expression.nullability.is_nullable());
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: self.types.boolean.clone(),
            typmod: None,
            nullability: if nullable {
                Nullability::nullable(NullabilityEvidence::Conservative)
            } else {
                synthetic_not_null("structural-operator")
            },
            volatility: max_volatility(
                arguments
                    .iter()
                    .map(|argument| argument.expression.volatility),
            ),
            kind: TypedExpressionKind::Operator {
                authored_operator_id: authored_id.clone(),
                operator_id: authored_id.clone(),
                operands: arguments,
            },
        })
    }

    fn select_operator<'a>(
        &self,
        candidates: Vec<&'a CatalogOperator>,
        actual: &[Option<TypeId>],
        authored_id: &OperatorId,
    ) -> Result<ResolvedCandidate<&'a CatalogOperator>, CheckError> {
        let compatible = self.select_pg_candidate(candidates, actual, |candidate| {
            candidate
                .left
                .iter()
                .chain(candidate.right.iter())
                .collect::<Vec<_>>()
        });
        compatible.map_err(|selection| match selection {
            SelectionError::None => CheckError::Type(TypeResolutionError::IncompatibleOperator {
                operator: authored_id.clone(),
                operand_types: actual.to_vec(),
            }),
            SelectionError::Ambiguous(candidates) => {
                CheckError::Type(TypeResolutionError::AmbiguousOperator {
                    name: operator_lookup_name(authored_id),
                    operand_types: actual.to_vec(),
                    candidates: candidates
                        .into_iter()
                        .map(|candidate| candidate.id.clone())
                        .collect(),
                })
            }
        })
    }

    fn check_call(
        &self,
        expression: &HirExpression,
        call: &HirCall,
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        let initial = call
            .arguments
            .iter()
            .map(|argument| self.check_expression(argument, context, None))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = initial
            .iter()
            .map(|argument| self.known_type(&argument.type_id))
            .collect::<Vec<_>>();
        let selected = self
            .select_pg_candidate(
                callable_candidates(self.catalog, &call.callable_id, call.arguments.len()),
                &actual,
                |candidate| candidate.arguments.iter().collect::<Vec<_>>(),
            )
            .map_err(|selection| match selection {
                SelectionError::None => {
                    CheckError::Type(TypeResolutionError::IncompatibleCallable {
                        name: callable_lookup_name(&call.callable_id),
                        argument_types: actual.clone(),
                    })
                }
                SelectionError::Ambiguous(candidates) => {
                    CheckError::Type(TypeResolutionError::AmbiguousCallable {
                        name: callable_lookup_name(&call.callable_id),
                        argument_types: actual.clone(),
                        candidates: candidates
                            .into_iter()
                            .map(|candidate| candidate.id.clone())
                            .collect(),
                    })
                }
            })?;
        let ResolvedCandidate {
            candidate: callable,
            argument_types: resolved_arguments,
        } = selected;
        if call.star && !(callable.kind == CallableKind::Aggregate && callable.arguments.is_empty())
        {
            return Err(TypeResolutionError::IncompatibleCallable {
                name: callable.qualified_name.clone(),
                argument_types: actual,
            }
            .into());
        }
        let declared_result = callable.scalar_result.as_ref().ok_or_else(|| {
            TypeResolutionError::MissingCatalogFact {
                kind: "callable-result",
                identity: callable.id.to_string(),
            }
        })?;
        let arguments = call
            .arguments
            .iter()
            .zip(&resolved_arguments)
            .map(|(argument, expected)| {
                self.check_argument(argument, context, expected, CoercionContext::Implicit)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let result = self.resolve_polymorphic_result(
            declared_result,
            &callable.arguments,
            &resolved_arguments,
        )?;
        let nullable_arguments = arguments
            .iter()
            .any(|argument| argument.expression.nullability.is_nullable());
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: result,
            typmod: None,
            nullability: callable_result_nullability(callable, expression.id, nullable_arguments),
            volatility: catalog_volatility(callable.volatility),
            kind: TypedExpressionKind::Call(Box::new(TypedCall {
                authored_callable_id: call.callable_id.clone(),
                callable_id: callable.id.clone(),
                arguments,
                distinct: call.distinct,
                star: call.star,
                order_by: call
                    .order_by
                    .iter()
                    .map(|order| self.check_order_by(order, context))
                    .collect::<Result<Vec<_>, _>>()?,
                filter: call
                    .filter
                    .as_deref()
                    .map(|value| self.check_predicate("FILTER", value, context))
                    .transpose()?
                    .map(Box::new),
                within_group: call
                    .within_group
                    .iter()
                    .map(|order| self.check_order_by(order, context))
                    .collect::<Result<Vec<_>, _>>()?,
                over: call
                    .over
                    .as_ref()
                    .map(|window| self.check_window_reference(window, context))
                    .transpose()?,
            })),
        })
    }

    pub(super) fn check_argument(
        &self,
        expression: &HirExpression,
        context: &CheckContext<'_>,
        expected: &TypeId,
        coercion_context: CoercionContext,
    ) -> Result<TypedArgument, CheckError> {
        let typed = self.check_expression(expression, context, Some(expected))?;
        if let HirExpressionKind::Literal(HirLiteral::Integer(value)) = &expression.kind
            && self.is_numeric(expected)
        {
            self.validate_numeric_literal(value, expected, true, &expression.origin)?;
        }
        if let HirExpressionKind::Literal(HirLiteral::Numeric(value)) = &expression.kind
            && self.is_numeric(expected)
        {
            self.validate_numeric_literal(value, expected, false, &expression.origin)?;
        }
        let coercion = self.coercion(&typed, expected, coercion_context)?;
        Ok(TypedArgument {
            expression: typed,
            coercion,
        })
    }

    fn check_explicit_cast(
        &self,
        expression: &HirExpression,
        cast_id: &dibs_pg_catalog::CastId,
        source: &HirExpression,
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        let cast = self
            .catalog
            .casts
            .iter()
            .find(|cast| &cast.id == cast_id)
            .ok_or_else(|| TypeResolutionError::MissingCatalogFact {
                kind: "cast",
                identity: cast_id.to_string(),
            })?;
        let source = self.check_expression(source, context, Some(&cast.source))?;
        let coercion = self
            .coercion(&source, &cast.target, CoercionContext::Explicit)?
            .ok_or_else(|| TypeResolutionError::MissingCatalogFact {
                kind: "explicit-cast-coercion",
                identity: cast.id.to_string(),
            })?;
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: cast.target.clone(),
            typmod: None,
            nullability: if source.nullability.is_nullable() {
                Nullability::nullable(NullabilityEvidence::CastPropagation)
            } else {
                Nullability::not_null(NullabilityEvidence::CastPropagation)
            },
            volatility: source.volatility,
            kind: TypedExpressionKind::Cast {
                cast_id: cast.id.clone(),
                expression: Box::new(source),
                coercion,
            },
        })
    }

    fn check_case(
        &self,
        expression: &HirExpression,
        operand: Option<&HirExpression>,
        branches: &[HirCaseBranch],
        else_expression: Option<&HirExpression>,
        context: &CheckContext<'_>,
        expected: Option<&TypeId>,
    ) -> Result<TypedExpression, CheckError> {
        let operand = operand
            .map(|value| self.check_expression(value, context, None))
            .transpose()?
            .map(Box::new);
        let typed_else = else_expression
            .map(|value| self.check_expression(value, context, None))
            .transpose()?;
        let mut branch_values = Vec::with_capacity(branches.len());
        let mut result_types = Vec::with_capacity(branches.len() + 1);
        result_types.push(
            typed_else
                .as_ref()
                .map_or_else(|| self.types.unknown.clone(), |value| value.type_id.clone()),
        );
        for branch in branches {
            let when = if let Some(operand) = &operand {
                self.check_expression(&branch.when, context, Some(&operand.type_id))?
            } else {
                self.check_predicate("CASE WHEN", &branch.when, context)?
            };
            let then = self.check_expression(&branch.then, context, None)?;
            result_types.push(then.type_id.clone());
            branch_values.push((when, then));
        }
        let result_type = expected
            .cloned()
            .map_or_else(|| self.common_type(&result_types), Ok)?;
        let typed_branches = branch_values
            .into_iter()
            .map(|(when, expression)| {
                Ok(TypedCaseBranch {
                    when,
                    then: TypedArgument {
                        coercion: self.coercion(
                            &expression,
                            &result_type,
                            CoercionContext::Implicit,
                        )?,
                        expression,
                    },
                })
            })
            .collect::<Result<Vec<_>, CheckError>>()?;
        let typed_else = typed_else
            .map(|expression| {
                Ok::<TypedArgument, CheckError>(TypedArgument {
                    coercion: self.coercion(
                        &expression,
                        &result_type,
                        CoercionContext::Implicit,
                    )?,
                    expression,
                })
            })
            .transpose()?
            .map(Box::new);
        let nullable = typed_else.is_none()
            || typed_else
                .as_ref()
                .is_some_and(|value| value.expression.nullability.is_nullable())
            || typed_branches
                .iter()
                .any(|branch| branch.then.expression.nullability.is_nullable());
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: result_type.clone(),
            typmod: None,
            nullability: if nullable {
                Nullability::nullable(NullabilityEvidence::CaseBranch)
            } else {
                synthetic_not_null("case")
            },
            volatility: max_volatility(
                operand
                    .iter()
                    .map(|value| value.volatility)
                    .chain(typed_branches.iter().flat_map(|branch| {
                        [branch.when.volatility, branch.then.expression.volatility]
                    }))
                    .chain(typed_else.iter().map(|value| value.expression.volatility)),
            ),
            kind: TypedExpressionKind::Case {
                operand,
                branches: typed_branches,
                else_expression: typed_else,
                implicit_else_type: else_expression
                    .is_none()
                    .then(|| self.types.unknown.clone()),
                result_coercion: CoercionEvidence::CommonType {
                    resolved: result_type,
                    inputs: result_types,
                },
            },
        })
    }

    fn check_array(
        &self,
        expression: &HirExpression,
        elements: &[HirExpression],
        context: &CheckContext<'_>,
        expected: Option<&TypeId>,
    ) -> Result<TypedExpression, CheckError> {
        let expected_element = expected
            .and_then(|type_id| self.catalog.type_by_id(type_id))
            .and_then(|ty| ty.element_type.as_ref());
        if elements.is_empty() && expected_element.is_none() {
            return Err(TypeResolutionError::IndeterminateArrayType.into());
        }
        let typed_elements = elements
            .iter()
            .map(|element| self.check_expression(element, context, None))
            .collect::<Result<Vec<_>, _>>()?;
        let input_types = typed_elements
            .iter()
            .map(|element| element.type_id.clone())
            .collect::<Vec<_>>();
        let element_type = expected_element
            .cloned()
            .map_or_else(|| self.common_type(&input_types), Ok)?;
        let volatility = max_volatility(typed_elements.iter().map(|element| element.volatility));
        let typed_elements = typed_elements
            .into_iter()
            .map(|expression| {
                Ok(TypedArgument {
                    coercion: self.coercion(
                        &expression,
                        &element_type,
                        CoercionContext::Implicit,
                    )?,
                    expression,
                })
            })
            .collect::<Result<Vec<_>, CheckError>>()?;
        let array_type = expected.cloned().or_else(|| {
            self.catalog
                .types
                .iter()
                .find(|ty| ty.element_type.as_ref() == Some(&element_type))
                .map(|ty| ty.id.clone())
        });
        let array_type = array_type.ok_or_else(|| TypeResolutionError::MissingCatalogFact {
            kind: "array-type",
            identity: element_type.to_string(),
        })?;
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: array_type,
            typmod: None,
            nullability: synthetic_not_null("array-constructor"),
            volatility,
            kind: TypedExpressionKind::Array {
                elements: typed_elements,
                coercion: CoercionEvidence::CommonType {
                    resolved: element_type,
                    inputs: input_types,
                },
            },
        })
    }

    pub(super) fn check_order_by(
        &self,
        order: &HirOrderBy,
        context: &CheckContext<'_>,
    ) -> Result<TypedOrderBy, CheckError> {
        Ok(TypedOrderBy {
            expression: self.check_expression(&order.expression, context, None)?,
            direction: order.direction,
            nulls: order.nulls,
        })
    }

    pub(super) fn check_named_window(
        &self,
        window: &HirNamedWindow,
        context: &CheckContext<'_>,
    ) -> Result<TypedNamedWindow, CheckError> {
        Ok(TypedNamedWindow {
            name: window.name.clone(),
            specification: self.check_window_spec(&window.specification, context)?,
        })
    }

    fn check_window_reference(
        &self,
        window: &WindowReference<HirExpression>,
        context: &CheckContext<'_>,
    ) -> Result<WindowReference<TypedExpression>, CheckError> {
        match window {
            WindowReference::Named(name) => Ok(WindowReference::Named(name.clone())),
            WindowReference::Inline(specification) => Ok(WindowReference::Inline(
                self.check_window_spec(specification, context)?,
            )),
        }
    }

    fn check_window_spec(
        &self,
        window: &WindowSpec<HirExpression>,
        context: &CheckContext<'_>,
    ) -> Result<WindowSpec<TypedExpression>, CheckError> {
        Ok(WindowSpec {
            existing: window.existing.clone(),
            partition_by: window
                .partition_by
                .iter()
                .map(|value| self.check_expression(value, context, None))
                .collect::<Result<Vec<_>, _>>()?,
            order_by: window
                .order_by
                .iter()
                .map(|order| self.check_order_by(order, context))
                .collect::<Result<Vec<_>, _>>()?,
            frame: window
                .frame
                .as_ref()
                .map(|frame| self.check_window_frame(frame, context))
                .transpose()?,
        })
    }

    fn check_window_frame(
        &self,
        frame: &WindowFrame<HirExpression>,
        context: &CheckContext<'_>,
    ) -> Result<WindowFrame<TypedExpression>, CheckError> {
        Ok(WindowFrame {
            mode: frame.mode,
            start: self.check_frame_bound(&frame.start, context)?,
            end: frame
                .end
                .as_ref()
                .map(|bound| self.check_frame_bound(bound, context))
                .transpose()?,
            exclusion: frame.exclusion,
        })
    }

    fn check_frame_bound(
        &self,
        bound: &FrameBound<HirExpression>,
        context: &CheckContext<'_>,
    ) -> Result<FrameBound<TypedExpression>, CheckError> {
        match bound {
            FrameBound::UnboundedPreceding => Ok(FrameBound::UnboundedPreceding),
            FrameBound::Preceding(value) => Ok(FrameBound::Preceding(self.check_expression(
                value,
                context,
                Some(&self.types.bigint),
            )?)),
            FrameBound::CurrentRow => Ok(FrameBound::CurrentRow),
            FrameBound::Following(value) => Ok(FrameBound::Following(self.check_expression(
                value,
                context,
                Some(&self.types.bigint),
            )?)),
            FrameBound::UnboundedFollowing => Ok(FrameBound::UnboundedFollowing),
        }
    }
}
fn valid_postgres_numeric_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = usize::from(matches!(bytes[0], b'+' | b'-'));
    let mut integral_digits = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        integral_digits += 1;
        index += 1;
    }
    let mut fractional_digits = 0usize;
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            fractional_digits += 1;
            index += 1;
        }
    }
    if integral_digits + fractional_digits == 0 {
        return false;
    }
    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        index += 1;
        if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
            index += 1;
        }
        let exponent_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if exponent_start == index {
            return false;
        }
    }
    index == bytes.len()
}

fn structural_operator(id: &OperatorId) -> Option<StructuralOperator> {
    match id.as_str() {
        SYNTAX_AND_OPERATOR_ID => Some(StructuralOperator::And),
        SYNTAX_OR_OPERATOR_ID => Some(StructuralOperator::Or),
        SYNTAX_NOT_OPERATOR_ID => Some(StructuralOperator::Not),
        SYNTAX_IS_NULL_OPERATOR_ID => Some(StructuralOperator::IsNull),
        SYNTAX_IS_NOT_NULL_OPERATOR_ID => Some(StructuralOperator::IsNotNull),
        SYNTAX_IS_DISTINCT_FROM_OPERATOR_ID => Some(StructuralOperator::IsDistinctFrom),
        SYNTAX_IS_NOT_DISTINCT_FROM_OPERATOR_ID => Some(StructuralOperator::IsNotDistinctFrom),
        _ => None,
    }
}

fn operator_candidates<'a>(
    catalog: &'a CatalogSnapshot,
    id: &OperatorId,
    arity: usize,
) -> Vec<&'a CatalogOperator> {
    if let Some(exact) = catalog.operators.iter().find(|operator| &operator.id == id) {
        return vec![exact];
    }
    let name = operator_lookup_name(id);
    catalog
        .operators
        .iter()
        .filter(|operator| {
            (operator.qualified_name == name
                || operator.qualified_name.rsplit('.').next() == Some(name.as_str()))
                && operator.left.iter().chain(operator.right.iter()).count() == arity
        })
        .collect()
}

fn callable_candidates<'a>(
    catalog: &'a CatalogSnapshot,
    id: &CallableId,
    arity: usize,
) -> Vec<&'a CatalogCallable> {
    if let Some(exact) = catalog.callable_by_id(id) {
        return vec![exact];
    }
    let name = callable_lookup_name(id);
    catalog
        .callables
        .iter()
        .filter(|callable| {
            (callable.qualified_name == name
                || callable.qualified_name.rsplit('.').next() == Some(name.as_str()))
                && callable.arguments.len() == arity
        })
        .collect()
}

fn operator_lookup_name(id: &OperatorId) -> String {
    unresolved_lookup_name(id.as_str(), "operator")
}

fn callable_lookup_name(id: &CallableId) -> String {
    unresolved_lookup_name(id.as_str(), "function")
}

fn unresolved_lookup_name(id: &str, category: &str) -> String {
    id.strip_prefix(&format!("unresolved:{category}:"))
        .unwrap_or(id)
        .to_string()
}

fn callable_nullability(identity: &str, nullable: bool) -> Nullability {
    if nullable {
        Nullability::nullable(NullabilityEvidence::CallableContract {
            callable_id: CallableId::new(identity),
            proves_non_null: false,
        })
    } else {
        Nullability::not_null(NullabilityEvidence::CallableContract {
            callable_id: CallableId::new(identity),
            proves_non_null: true,
        })
    }
}

pub(super) fn catalog_volatility(volatility: CatalogVolatility) -> Volatility {
    match volatility {
        CatalogVolatility::Immutable => Volatility::Immutable,
        CatalogVolatility::Stable => Volatility::Stable,
        CatalogVolatility::Volatile => Volatility::Volatile,
    }
}

fn callable_result_nullability(
    callable: &CatalogCallable,
    expression: ExpressionId,
    nullable_arguments: bool,
) -> Nullability {
    if callable.kind == CallableKind::Aggregate
        && callable.aggregate_empty == Some(AggregateEmptyBehavior::Null)
    {
        return Nullability::nullable(NullabilityEvidence::AggregateEmptyInput { expression });
    }
    let nullable = callable.scalar_result_nullability == Some(CatalogNullability::Nullable)
        || (callable.strict && nullable_arguments);
    callable_nullability(callable.id.as_str(), nullable)
}

fn synthetic_not_null(kind: &str) -> Nullability {
    Nullability::not_null(NullabilityEvidence::SyntheticNonNull {
        kind: kind.to_owned(),
    })
}

fn synthetic_row_type(values: &[TypedExpression]) -> TypeId {
    TypeId::new(format!(
        "pg18:type:record:({})",
        values
            .iter()
            .map(|value| value.type_id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    ))
}

pub(super) fn max_volatility(values: impl IntoIterator<Item = Volatility>) -> Volatility {
    values.into_iter().max().unwrap_or(Volatility::Immutable)
}

pub(super) fn expression_has_scalar_aggregate(
    expression: &TypedExpression,
    catalog: &CatalogSnapshot,
) -> bool {
    match &expression.kind {
        TypedExpressionKind::Call(call) => {
            call.over.is_none()
                && catalog
                    .callable_by_id(&call.callable_id)
                    .is_some_and(|callable| callable.kind == CallableKind::Aggregate)
        }
        TypedExpressionKind::Operator { operands, .. } => operands
            .iter()
            .any(|argument| expression_has_scalar_aggregate(&argument.expression, catalog)),
        TypedExpressionKind::Cast { expression, .. }
        | TypedExpressionKind::Collate { expression, .. } => {
            expression_has_scalar_aggregate(expression, catalog)
        }
        TypedExpressionKind::Case {
            operand,
            branches,
            else_expression,
            ..
        } => {
            operand
                .as_deref()
                .is_some_and(|value| expression_has_scalar_aggregate(value, catalog))
                || branches.iter().any(|branch| {
                    expression_has_scalar_aggregate(&branch.when, catalog)
                        || expression_has_scalar_aggregate(&branch.then.expression, catalog)
                })
                || else_expression.as_deref().is_some_and(|value| {
                    expression_has_scalar_aggregate(&value.expression, catalog)
                })
        }
        TypedExpressionKind::Row(values) => values
            .iter()
            .any(|value| expression_has_scalar_aggregate(value, catalog)),
        TypedExpressionKind::Array { elements, .. } => elements
            .iter()
            .any(|value| expression_has_scalar_aggregate(&value.expression, catalog)),
        TypedExpressionKind::Literal(_)
        | TypedExpressionKind::Parameter(_)
        | TypedExpressionKind::Column { .. }
        | TypedExpressionKind::DerivedColumn { .. }
        | TypedExpressionKind::ScalarSubquery(_)
        | TypedExpressionKind::CteColumn { .. } => false,
    }
}

pub(super) fn expression_is_group_legal(
    expression: &TypedExpression,
    group_by: &[TypedExpression],
    catalog: &CatalogSnapshot,
) -> bool {
    if group_by
        .iter()
        .any(|group| expression_same_value(expression, group))
    {
        return true;
    }
    match &expression.kind {
        TypedExpressionKind::Call(call) => {
            if call.over.is_none()
                && catalog
                    .callable_by_id(&call.callable_id)
                    .is_some_and(|callable| callable.kind == CallableKind::Aggregate)
            {
                true
            } else {
                call.arguments.iter().all(|argument| {
                    expression_is_group_legal(&argument.expression, group_by, catalog)
                })
            }
        }
        TypedExpressionKind::Operator { operands, .. } => operands
            .iter()
            .all(|argument| expression_is_group_legal(&argument.expression, group_by, catalog)),
        TypedExpressionKind::Cast { expression, .. }
        | TypedExpressionKind::Collate { expression, .. } => {
            expression_is_group_legal(expression, group_by, catalog)
        }
        TypedExpressionKind::Case {
            operand,
            branches,
            else_expression,
            ..
        } => {
            operand
                .as_deref()
                .is_none_or(|value| expression_is_group_legal(value, group_by, catalog))
                && branches.iter().all(|branch| {
                    expression_is_group_legal(&branch.when, group_by, catalog)
                        && expression_is_group_legal(&branch.then.expression, group_by, catalog)
                })
                && else_expression.as_deref().is_none_or(|value| {
                    expression_is_group_legal(&value.expression, group_by, catalog)
                })
        }
        TypedExpressionKind::Row(values) => values
            .iter()
            .all(|value| expression_is_group_legal(value, group_by, catalog)),
        TypedExpressionKind::Array { elements, .. } => elements
            .iter()
            .all(|value| expression_is_group_legal(&value.expression, group_by, catalog)),
        TypedExpressionKind::Literal(_) | TypedExpressionKind::Parameter(_) => true,
        TypedExpressionKind::Column { .. }
        | TypedExpressionKind::DerivedColumn { .. }
        | TypedExpressionKind::CteColumn { .. } => false,
        TypedExpressionKind::ScalarSubquery(_) => true,
    }
}

pub(super) fn expression_same_value(left: &TypedExpression, right: &TypedExpression) -> bool {
    match (&left.kind, &right.kind) {
        (
            TypedExpressionKind::Column {
                binding: left_binding,
                column_id: left_column,
            },
            TypedExpressionKind::Column {
                binding: right_binding,
                column_id: right_column,
            },
        ) => left_binding == right_binding && left_column == right_column,
        (
            TypedExpressionKind::DerivedColumn {
                binding: left_binding,
                field_id: left_field,
            },
            TypedExpressionKind::DerivedColumn {
                binding: right_binding,
                field_id: right_field,
            },
        ) => left_binding == right_binding && left_field == right_field,
        (
            TypedExpressionKind::CteColumn {
                cte_id: left_cte,
                field_id: left_field,
            },
            TypedExpressionKind::CteColumn {
                cte_id: right_cte,
                field_id: right_field,
            },
        ) => left_cte == right_cte && left_field == right_field,
        (TypedExpressionKind::Parameter(left), TypedExpressionKind::Parameter(right)) => {
            left == right
        }
        (TypedExpressionKind::Literal(left), TypedExpressionKind::Literal(right)) => left == right,
        (TypedExpressionKind::Call(left), TypedExpressionKind::Call(right)) => {
            left.callable_id == right.callable_id
                && left.distinct == right.distinct
                && left.star == right.star
                && arguments_same_value(&left.arguments, &right.arguments)
                && orderings_same_value(&left.order_by, &right.order_by)
                && optional_expression_same_value(left.filter.as_deref(), right.filter.as_deref())
                && orderings_same_value(&left.within_group, &right.within_group)
                && left.over == right.over
        }
        (
            TypedExpressionKind::Operator {
                operator_id: left_operator,
                operands: left_operands,
                ..
            },
            TypedExpressionKind::Operator {
                operator_id: right_operator,
                operands: right_operands,
                ..
            },
        ) => left_operator == right_operator && arguments_same_value(left_operands, right_operands),
        (
            TypedExpressionKind::Cast {
                cast_id: left_cast,
                expression: left_expression,
                coercion: left_coercion,
            },
            TypedExpressionKind::Cast {
                cast_id: right_cast,
                expression: right_expression,
                coercion: right_coercion,
            },
        ) => {
            left_cast == right_cast
                && left_coercion == right_coercion
                && expression_same_value(left_expression, right_expression)
        }
        (
            TypedExpressionKind::Collate {
                collation_id: left_collation,
                expression: left_expression,
            },
            TypedExpressionKind::Collate {
                collation_id: right_collation,
                expression: right_expression,
            },
        ) => {
            left_collation == right_collation
                && expression_same_value(left_expression, right_expression)
        }
        (TypedExpressionKind::Row(left), TypedExpressionKind::Row(right)) => {
            expressions_same_value(left, right)
        }
        (
            TypedExpressionKind::Array {
                elements: left,
                coercion: left_coercion,
            },
            TypedExpressionKind::Array {
                elements: right,
                coercion: right_coercion,
            },
        ) => left_coercion == right_coercion && arguments_same_value(left, right),
        _ => false,
    }
}

fn expressions_same_value(left: &[TypedExpression], right: &[TypedExpression]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| expression_same_value(left, right))
}

fn arguments_same_value(left: &[TypedArgument], right: &[TypedArgument]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.coercion == right.coercion
                && expression_same_value(&left.expression, &right.expression)
        })
}

fn orderings_same_value(left: &[TypedOrderBy], right: &[TypedOrderBy]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.direction == right.direction
                && left.nulls == right.nulls
                && expression_same_value(&left.expression, &right.expression)
        })
}

fn optional_expression_same_value(
    left: Option<&TypedExpression>,
    right: Option<&TypedExpression>,
) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => expression_same_value(left, right),
        (None, None) => true,
        _ => false,
    }
}
