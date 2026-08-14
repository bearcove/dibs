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
            HirExpressionKind::QuantifiedComparison {
                operator_id,
                left,
                right,
                quantifier,
            } => self.check_quantified_comparison(
                expression,
                operator_id,
                left,
                right,
                *quantifier,
                context,
            ),
            HirExpressionKind::InList {
                expression: operand,
                values,
                negated,
            } => self.check_in_list(expression, operand, values, *negated, context),

            HirExpressionKind::Cast {
                cast_id,
                expression: source,
            } => self.check_explicit_cast(expression, cast_id, source, context),
            HirExpressionKind::ExplicitCast {
                target_type,
                target_typmod,
                expression: source,
            } => self.check_authored_explicit_cast(
                expression,
                target_type,
                target_typmod.as_ref(),
                source,
                context,
            ),
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
            HirExpressionKind::Coalesce(arguments) => {
                self.check_coalesce(expression, arguments, context, expected)
            }
            HirExpressionKind::NullIf { left, right } => {
                self.check_nullif(expression, left, right, context)
            }
            HirExpressionKind::Greatest(arguments) => self.check_common_type_special_form(
                expression,
                arguments,
                context,
                expected,
                CommonTypeSpecialForm::Greatest,
            ),
            HirExpressionKind::Least(arguments) => self.check_common_type_special_form(
                expression,
                arguments,
                context,
                expected,
                CommonTypeSpecialForm::Least,
            ),
            HirExpressionKind::Position { substring, string } => {
                let typed_substring = self.check_expression(substring, context, None)?;
                let typed_string = self.check_expression(string, context, None)?;
                let input_type = self.common_type(&[
                    typed_substring.type_id.clone(),
                    typed_string.type_id.clone(),
                ])?;
                if input_type != self.types.text && input_type != self.types.bytea {
                    return Err(TypeResolutionError::IncompatibleCommonType {
                        types: vec![
                            self.known_type(&typed_substring.type_id),
                            self.known_type(&typed_string.type_id),
                        ],
                    }
                    .into());
                }
                let substring = self.check_argument(
                    substring,
                    context,
                    &input_type,
                    CoercionContext::Implicit,
                )?;
                let string =
                    self.check_argument(string, context, &input_type, CoercionContext::Implicit)?;
                let nullable = substring.expression.nullability.is_nullable()
                    || string.expression.nullability.is_nullable();
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: self.types.integer.clone(),
                    typmod: None,
                    nullability: if nullable {
                        Nullability::nullable(NullabilityEvidence::Conservative)
                    } else {
                        synthetic_not_null("position")
                    },
                    volatility: max_volatility([
                        substring.expression.volatility,
                        string.expression.volatility,
                    ]),
                    kind: TypedExpressionKind::Position {
                        substring: Box::new(substring),
                        string: Box::new(string),
                        input_type,
                    },
                })
            }
            HirExpressionKind::Exists(statement) => {
                let mut nested = context.clone();
                let statement = self.check_statement(statement, &mut nested)?;
                let volatility = statement_volatility(&statement);
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: self.types.boolean.clone(),
                    typmod: None,
                    nullability: synthetic_not_null("exists"),
                    volatility,
                    kind: TypedExpressionKind::Exists(Box::new(statement)),
                })
            }
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
            HirExpressionKind::Extract { field, source } => {
                let source = self.check_expression(source, context, None)?;
                let source_type = self.catalog.type_by_id(&source.type_id).ok_or_else(|| {
                    TypeResolutionError::MissingCatalogFact {
                        kind: "type",
                        identity: source.type_id.to_string(),
                    }
                })?;
                if !matches!(
                    source_type.category,
                    PgTypeCategory::DateTime | PgTypeCategory::Timespan
                ) {
                    return Err(TypeResolutionError::MissingCatalogFact {
                        kind: "extract-source",
                        identity: source.type_id.to_string(),
                    }
                    .into());
                }
                Ok(TypedExpression {
                    id: expression.id,
                    origin: expression.origin.clone(),
                    type_id: self.types.numeric.clone(),
                    typmod: None,
                    nullability: source.nullability.clone(),
                    volatility: source.volatility,
                    kind: TypedExpressionKind::Extract {
                        field: *field,
                        source: Box::new(source),
                    },
                })
            }
            HirExpressionKind::CteColumn {
                cte_id,
                binding,
                field_id,
            } => {
                if context.cte_bindings.get(binding) != Some(cte_id) {
                    return Err(CheckError::UnknownColumn {
                        binding: *binding,
                        column_id: synthetic_field_column(*binding, *field_id),
                        origin: expression.origin.clone(),
                    });
                }
                let value = context
                    .ctes
                    .get(cte_id)
                    .and_then(|cte| cte.fields.get(field_id))
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
                    nullability: if context.null_extended.contains(binding)
                        || value.nullability.is_nullable()
                    {
                        Nullability::nullable(if context.null_extended.contains(binding) {
                            NullabilityEvidence::OuterJoinNullExtension { binding: *binding }
                        } else {
                            NullabilityEvidence::CtePropagation { cte: *cte_id }
                        })
                    } else {
                        Nullability::not_null(NullabilityEvidence::CtePropagation { cte: *cte_id })
                    },
                    volatility: value.volatility,
                    kind: TypedExpressionKind::CteColumn {
                        cte_id: *cte_id,
                        binding: *binding,
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
            HirLiteral::Interval { .. } => (
                self.types.interval.clone(),
                synthetic_not_null("interval-literal"),
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

    fn check_in_list(
        &self,
        expression: &HirExpression,
        operand: &HirExpression,
        values: &[HirExpression],
        negated: bool,
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        let typed_operand = self.check_expression(operand, context, None)?;
        let typed_values = values
            .iter()
            .map(|value| self.check_expression(value, context, None))
            .collect::<Result<Vec<_>, _>>()?;
        let input_types = std::iter::once(typed_operand.type_id.clone())
            .chain(typed_values.iter().map(|value| value.type_id.clone()))
            .collect::<Vec<_>>();
        let common = self.common_type(&input_types)?;
        let nullable = typed_operand.nullability.is_nullable()
            || typed_values
                .iter()
                .any(|value| value.nullability.is_nullable());
        let volatility = max_volatility(
            std::iter::once(typed_operand.volatility)
                .chain(typed_values.iter().map(|value| value.volatility)),
        );
        let typed_operand = TypedArgument {
            coercion: self.coercion(&typed_operand, &common, CoercionContext::Implicit)?,
            expression: typed_operand,
        };
        let typed_values = typed_values
            .into_iter()
            .map(|value| {
                Ok(TypedArgument {
                    coercion: self.coercion(&value, &common, CoercionContext::Implicit)?,
                    expression: value,
                })
            })
            .collect::<Result<Vec<_>, CheckError>>()?;
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: self.types.boolean.clone(),
            typmod: None,
            nullability: if nullable {
                Nullability::nullable(NullabilityEvidence::Conservative)
            } else {
                synthetic_not_null("in-list")
            },
            volatility,
            kind: TypedExpressionKind::InList {
                expression: Box::new(typed_operand),
                values: typed_values,
                negated,
                coercion: CoercionEvidence::CommonType {
                    resolved: common,
                    inputs: input_types,
                },
            },
        })
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

    fn check_quantified_comparison(
        &self,
        expression: &HirExpression,
        authored_id: &OperatorId,
        left: &HirExpression,
        right: &HirExpression,
        quantifier: ComparisonQuantifier,
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        let typed_left = self.check_expression(left, context, None)?;
        let typed_right = self.check_expression(right, context, None)?;
        let right_element_type = self
            .catalog
            .type_by_id(&typed_right.type_id)
            .and_then(|facts| facts.element_type.clone())
            .ok_or_else(|| TypeResolutionError::MissingCatalogFact {
                kind: "array-element-type",
                identity: typed_right.type_id.to_string(),
            })?;
        let actual = vec![
            self.known_type(&typed_left.type_id),
            self.known_type(&right_element_type),
        ];
        let selected = self.select_operator(
            operator_candidates(self.catalog, authored_id, 2),
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
        let result =
            self.resolve_polymorphic_result(&operator.result, &declared, &argument_types)?;
        if result != self.types.boolean {
            return Err(CheckError::NonBooleanPredicate {
                clause: "quantified comparison",
                actual: result,
                origin: expression.origin.clone(),
            });
        }
        let expected_array = self
            .catalog
            .types
            .iter()
            .find(|facts| facts.element_type.as_ref() == argument_types.get(1))
            .map(|facts| facts.id.clone())
            .ok_or_else(|| TypeResolutionError::MissingCatalogFact {
                kind: "array-type",
                identity: argument_types[1].to_string(),
            })?;
        let left = TypedArgument {
            coercion: self.coercion(&typed_left, &argument_types[0], CoercionContext::Implicit)?,
            expression: typed_left,
        };
        let right = TypedArgument {
            coercion: self.coercion(&typed_right, &expected_array, CoercionContext::Implicit)?,
            expression: typed_right,
        };
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: self.types.boolean.clone(),
            typmod: None,
            nullability: Nullability::nullable(NullabilityEvidence::Conservative),
            volatility: max_volatility([left.expression.volatility, right.expression.volatility]),
            kind: TypedExpressionKind::QuantifiedComparison {
                authored_operator_id: authored_id.clone(),
                operator_id: operator.id.clone(),
                left: Box::new(left),
                right: Box::new(right),
                quantifier,
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
        let initial_within_group = call
            .within_group
            .iter()
            .map(|order| self.check_expression(&order.expression, context, None))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = initial
            .iter()
            .chain(&initial_within_group)
            .map(|argument| self.known_type(&argument.type_id))
            .collect::<Vec<_>>();
        let candidates = callable_candidates(self.catalog, &call.callable_id, call.arguments.len())
            .into_iter()
            .filter(|callable| callable.aggregated_arguments.len() == call.within_group.len())
            .filter_map(|callable| {
                callable_argument_types(callable, &call.argument_names).map(|argument_types| {
                    CallableCandidate {
                        callable,
                        argument_types,
                    }
                })
            })
            .collect::<Vec<_>>();
        let selected = self
            .select_pg_candidate(&candidates, &actual, |candidate| {
                candidate
                    .argument_types
                    .iter()
                    .chain(&candidate.callable.aggregated_arguments)
                    .collect::<Vec<_>>()
            })
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
                            .map(|candidate| candidate.callable.id.clone())
                            .collect(),
                    })
                }
            })?;
        let ResolvedCandidate {
            candidate,
            argument_types: resolved_types,
        } = selected;
        let callable = candidate.callable;
        let direct_count = candidate.argument_types.len();
        let (resolved_arguments, resolved_aggregated_arguments) =
            resolved_types.split_at(direct_count);
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
            .zip(resolved_arguments)
            .map(|(argument, expected)| {
                self.check_argument(argument, context, expected, CoercionContext::Implicit)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let within_group = call
            .within_group
            .iter()
            .zip(resolved_aggregated_arguments)
            .map(|(order, expected)| {
                Ok(TypedWithinGroupOrderBy {
                    expression: self.check_argument(
                        &order.expression,
                        context,
                        expected,
                        CoercionContext::Implicit,
                    )?,
                    direction: order.direction,
                    nulls: order.nulls,
                })
            })
            .collect::<Result<Vec<_>, CheckError>>()?;
        let declared_types = callable
            .arguments
            .iter()
            .chain(&callable.aggregated_arguments)
            .cloned()
            .collect::<Vec<_>>();
        let result =
            self.resolve_polymorphic_result(declared_result, &declared_types, &resolved_types)?;
        let nullable_arguments = arguments
            .iter()
            .map(|argument| &argument.expression)
            .chain(
                within_group
                    .iter()
                    .map(|order| &order.expression.expression),
            )
            .any(|expression| expression.nullability.is_nullable());
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
                argument_names: call.argument_names.clone(),
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
                within_group,
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

    fn check_authored_explicit_cast(
        &self,
        expression: &HirExpression,
        target_type: &TypeId,
        target_typmod: Option<&Typmod>,
        source: &HirExpression,
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        let source = self.check_expression(source, context, None)?;
        let mut coercion = self.coercion(&source, target_type, CoercionContext::Explicit)?;
        if let Some(coercion) = &mut coercion {
            coercion.target_typmod = target_typmod.cloned();
        }
        let identity_without_typmod = source.type_id == *target_type && target_typmod.is_none();
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: target_type.clone(),
            typmod: target_typmod.cloned(),
            nullability: if source.nullability.is_nullable() {
                Nullability::nullable(NullabilityEvidence::CastPropagation)
            } else {
                Nullability::not_null(NullabilityEvidence::CastPropagation)
            },
            volatility: source.volatility,
            kind: TypedExpressionKind::ExplicitCast {
                expression: Box::new(source),
                coercion: if identity_without_typmod {
                    None
                } else {
                    coercion
                },
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

    fn check_nullif(
        &self,
        expression: &HirExpression,
        left: &HirExpression,
        right: &HirExpression,
        context: &CheckContext<'_>,
    ) -> Result<TypedExpression, CheckError> {
        let authored_operator_id = OperatorId::new("unresolved:operator:pg_catalog.=");
        let typed_left = self.check_expression(left, context, None)?;
        let typed_right = self.check_expression(right, context, None)?;
        let actual = vec![
            self.known_type(&typed_left.type_id),
            self.known_type(&typed_right.type_id),
        ];
        let selected = self.select_operator(
            operator_candidates(self.catalog, &authored_operator_id, 2),
            &actual,
            &authored_operator_id,
        )?;
        let ResolvedCandidate {
            candidate: operator,
            argument_types,
        } = selected;
        let left = TypedArgument {
            coercion: self.coercion(&typed_left, &argument_types[0], CoercionContext::Implicit)?,
            expression: typed_left,
        };
        let right = TypedArgument {
            coercion: self.coercion(&typed_right, &argument_types[1], CoercionContext::Implicit)?,
            expression: typed_right,
        };
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: argument_types[0].clone(),
            typmod: argument_output_typmod(&left).cloned(),
            nullability: Nullability::nullable(NullabilityEvidence::Conservative),
            volatility: max_volatility([left.expression.volatility, right.expression.volatility]),
            kind: TypedExpressionKind::NullIf {
                authored_operator_id,
                operator_id: operator.id.clone(),
                left: Box::new(left),
                right: Box::new(right),
            },
        })
    }

    fn check_coalesce(
        &self,
        expression: &HirExpression,
        arguments: &[HirExpression],
        context: &CheckContext<'_>,
        expected: Option<&TypeId>,
    ) -> Result<TypedExpression, CheckError> {
        let values = arguments
            .iter()
            .map(|argument| self.check_expression(argument, context, None))
            .collect::<Result<Vec<_>, _>>()?;
        let input_types = values
            .iter()
            .map(|value| value.type_id.clone())
            .collect::<Vec<_>>();
        let result_type = expected
            .cloned()
            .map_or_else(|| self.common_type(&input_types), Ok)?;
        let nullable = values.iter().all(|value| value.nullability.is_nullable());
        let volatility = max_volatility(values.iter().map(|value| value.volatility));
        let arguments = values
            .into_iter()
            .map(|value| {
                Ok(TypedArgument {
                    coercion: self.coercion(&value, &result_type, CoercionContext::Implicit)?,
                    expression: value,
                })
            })
            .collect::<Result<Vec<_>, CheckError>>()?;
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: result_type.clone(),
            typmod: None,
            nullability: if nullable {
                Nullability::nullable(NullabilityEvidence::Conservative)
            } else {
                synthetic_not_null("coalesce")
            },
            volatility,
            kind: TypedExpressionKind::Coalesce {
                arguments,
                coercion: CoercionEvidence::CommonType {
                    resolved: result_type,
                    inputs: input_types,
                },
            },
        })
    }
    fn check_common_type_special_form(
        &self,
        expression: &HirExpression,
        arguments: &[HirExpression],
        context: &CheckContext<'_>,
        expected: Option<&TypeId>,
        form: CommonTypeSpecialForm,
    ) -> Result<TypedExpression, CheckError> {
        let values = arguments
            .iter()
            .map(|argument| self.check_expression(argument, context, None))
            .collect::<Result<Vec<_>, _>>()?;
        let input_types = values
            .iter()
            .map(|value| value.type_id.clone())
            .collect::<Vec<_>>();
        let result_type = expected
            .cloned()
            .map_or_else(|| self.common_type(&input_types), Ok)?;
        let nullability = if values.iter().all(|value| value.nullability.is_nullable()) {
            Nullability::nullable(NullabilityEvidence::Conservative)
        } else {
            synthetic_not_null(form.name())
        };
        let volatility = max_volatility(values.iter().map(|value| value.volatility));
        let arguments = values
            .into_iter()
            .map(|value| {
                Ok(TypedArgument {
                    coercion: self.coercion(&value, &result_type, CoercionContext::Implicit)?,
                    expression: value,
                })
            })
            .collect::<Result<Vec<_>, CheckError>>()?;
        let coercion = CoercionEvidence::CommonType {
            resolved: result_type.clone(),
            inputs: input_types,
        };
        let kind = match form {
            CommonTypeSpecialForm::Greatest => TypedExpressionKind::Greatest {
                arguments,
                coercion,
            },
            CommonTypeSpecialForm::Least => TypedExpressionKind::Least {
                arguments,
                coercion,
            },
        };
        Ok(TypedExpression {
            id: expression.id,
            origin: expression.origin.clone(),
            type_id: result_type,
            typmod: None,
            nullability,
            volatility,
            kind,
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
#[derive(Debug, Clone, Copy)]
enum CommonTypeSpecialForm {
    Greatest,
    Least,
}

impl CommonTypeSpecialForm {
    const fn name(self) -> &'static str {
        match self {
            Self::Greatest => "greatest",
            Self::Least => "least",
        }
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

struct CallableCandidate<'a> {
    callable: &'a CatalogCallable,
    argument_types: Vec<TypeId>,
}

fn callable_candidates<'a>(
    catalog: &'a CatalogSnapshot,
    id: &CallableId,
    arity: usize,
) -> Vec<&'a CatalogCallable> {
    if let Some(exact) = catalog.callable_by_id(id) {
        return ((exact.required_arguments <= arity) && (arity <= exact.arguments.len()))
            .then_some(exact)
            .into_iter()
            .collect();
    }
    let name = callable_lookup_name(id);
    catalog
        .callables
        .iter()
        .filter(|callable| {
            (callable.qualified_name == name
                || callable.qualified_name.rsplit('.').next() == Some(name.as_str()))
                && callable.required_arguments <= arity
                && arity <= callable.arguments.len()
        })
        .collect()
}

fn callable_argument_types(
    callable: &CatalogCallable,
    authored_names: &[Option<String>],
) -> Option<Vec<TypeId>> {
    let mut used = vec![false; callable.arguments.len()];
    let mut next_positional = 0;
    let mut saw_named = false;
    let mut argument_types = Vec::with_capacity(authored_names.len());
    for authored_name in authored_names {
        let position = if let Some(name) = authored_name {
            saw_named = true;
            callable
                .parameter_names
                .iter()
                .position(|candidate| candidate.as_deref() == Some(name.as_str()))?
        } else {
            if saw_named {
                return None;
            }
            let position = next_positional;
            next_positional += 1;
            position
        };
        if position >= callable.arguments.len() || used[position] {
            return None;
        }
        used[position] = true;
        argument_types.push(callable.arguments[position].clone());
    }
    if used
        .iter()
        .take(callable.required_arguments)
        .any(|used| !used)
    {
        return None;
    }
    Some(argument_types)
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

fn argument_output_typmod(argument: &TypedArgument) -> Option<&Typmod> {
    argument
        .coercion
        .as_ref()
        .map_or(argument.expression.typmod.as_ref(), |coercion| {
            coercion.target_typmod.as_ref()
        })
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
        TypedExpressionKind::Extract { source, .. } => {
            expression_has_scalar_aggregate(source, catalog)
        }
        TypedExpressionKind::Operator { operands, .. } => operands
            .iter()
            .any(|argument| expression_has_scalar_aggregate(&argument.expression, catalog)),
        TypedExpressionKind::QuantifiedComparison { left, right, .. } => {
            expression_has_scalar_aggregate(&left.expression, catalog)
                || expression_has_scalar_aggregate(&right.expression, catalog)
        }
        TypedExpressionKind::NullIf { left, right, .. } => {
            expression_has_scalar_aggregate(&left.expression, catalog)
                || expression_has_scalar_aggregate(&right.expression, catalog)
        }
        TypedExpressionKind::InList {
            expression, values, ..
        } => {
            expression_has_scalar_aggregate(&expression.expression, catalog)
                || values
                    .iter()
                    .any(|value| expression_has_scalar_aggregate(&value.expression, catalog))
        }
        TypedExpressionKind::Cast { expression, .. }
        | TypedExpressionKind::ExplicitCast { expression, .. }
        | TypedExpressionKind::Collate { expression, .. } => {
            expression_has_scalar_aggregate(expression, catalog)
        }
        TypedExpressionKind::Position {
            substring, string, ..
        } => {
            expression_has_scalar_aggregate(&substring.expression, catalog)
                || expression_has_scalar_aggregate(&string.expression, catalog)
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
        TypedExpressionKind::Coalesce { arguments, .. }
        | TypedExpressionKind::Greatest { arguments, .. }
        | TypedExpressionKind::Least { arguments, .. } => arguments
            .iter()
            .any(|argument| expression_has_scalar_aggregate(&argument.expression, catalog)),
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
        | TypedExpressionKind::Exists(_)
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
        TypedExpressionKind::Extract { source, .. } => {
            expression_is_group_legal(source, group_by, catalog)
        }
        TypedExpressionKind::Operator { operands, .. } => operands
            .iter()
            .all(|argument| expression_is_group_legal(&argument.expression, group_by, catalog)),
        TypedExpressionKind::QuantifiedComparison { left, right, .. } => {
            expression_is_group_legal(&left.expression, group_by, catalog)
                && expression_is_group_legal(&right.expression, group_by, catalog)
        }
        TypedExpressionKind::NullIf { left, right, .. } => {
            expression_is_group_legal(&left.expression, group_by, catalog)
                && expression_is_group_legal(&right.expression, group_by, catalog)
        }
        TypedExpressionKind::InList {
            expression, values, ..
        } => {
            expression_is_group_legal(&expression.expression, group_by, catalog)
                && values
                    .iter()
                    .all(|value| expression_is_group_legal(&value.expression, group_by, catalog))
        }
        TypedExpressionKind::Position {
            substring, string, ..
        } => {
            expression_is_group_legal(&substring.expression, group_by, catalog)
                && expression_is_group_legal(&string.expression, group_by, catalog)
        }
        TypedExpressionKind::Cast { expression, .. }
        | TypedExpressionKind::ExplicitCast { expression, .. }
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
        TypedExpressionKind::Coalesce { arguments, .. }
        | TypedExpressionKind::Greatest { arguments, .. }
        | TypedExpressionKind::Least { arguments, .. } => arguments
            .iter()
            .all(|argument| expression_is_group_legal(&argument.expression, group_by, catalog)),
        TypedExpressionKind::Row(values) => values
            .iter()
            .all(|value| expression_is_group_legal(value, group_by, catalog)),
        TypedExpressionKind::Array { elements, .. } => elements
            .iter()
            .all(|value| expression_is_group_legal(&value.expression, group_by, catalog)),
        TypedExpressionKind::Literal(_)
        | TypedExpressionKind::Parameter(_)
        | TypedExpressionKind::Exists(_) => true,
        TypedExpressionKind::Column { binding, column_id } => {
            column_is_functionally_grouped(*binding, column_id, group_by, catalog)
        }
        TypedExpressionKind::DerivedColumn { .. } | TypedExpressionKind::CteColumn { .. } => false,
        TypedExpressionKind::ScalarSubquery(_) => true,
    }
}

fn column_is_functionally_grouped(
    binding: RelationId,
    column_id: &dibs_pg_catalog::ColumnId,
    group_by: &[TypedExpression],
    catalog: &CatalogSnapshot,
) -> bool {
    let Some(table) = catalog
        .tables
        .iter()
        .find(|table| table.columns.iter().any(|column| &column.id == column_id))
    else {
        return false;
    };
    !table.primary_key.columns.is_empty()
        && table.primary_key.columns.iter().all(|key_name| {
            let Some(key_column) = table.column(key_name) else {
                return false;
            };
            group_by.iter().any(|group| {
                matches!(
                    &group.kind,
                    TypedExpressionKind::Column {
                        binding: grouped_binding,
                        column_id: grouped_column,
                    } if *grouped_binding == binding && grouped_column == &key_column.id
                )
            })
        })
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
                binding: left_binding,
                field_id: left_field,
            },
            TypedExpressionKind::CteColumn {
                cte_id: right_cte,
                binding: right_binding,
                field_id: right_field,
            },
        ) => left_cte == right_cte && left_binding == right_binding && left_field == right_field,
        (TypedExpressionKind::Parameter(left), TypedExpressionKind::Parameter(right)) => {
            left == right
        }
        (TypedExpressionKind::Literal(left), TypedExpressionKind::Literal(right)) => left == right,
        _ => false,
    }
}

fn statement_volatility(statement: &TypedStatement) -> Volatility {
    match &statement.kind {
        TypedStatementKind::Select(select) => max_volatility(
            select
                .ctes
                .iter()
                .map(|cte| statement_volatility(&cte.statement))
                .chain(select.projections.iter().map(|p| p.expression.volatility))
                .chain(select.from.iter().map(relation_volatility))
                .chain(select.predicate.iter().map(|e| e.volatility))
                .chain(select.group_by.iter().map(|e| e.volatility))
                .chain(select.having.iter().map(|e| e.volatility))
                .chain(select.order_by.iter().map(|o| o.expression.volatility)),
        ),
        TypedStatementKind::Insert(insert) => max_volatility(
            insert
                .ctes
                .iter()
                .map(|cte| statement_volatility(&cte.statement))
                .chain(insert_source_volatility(&insert.source))
                .chain(insert.returning.iter().map(|p| p.expression.volatility)),
        ),
        TypedStatementKind::Update(update) => max_volatility(
            update
                .ctes
                .iter()
                .map(|cte| statement_volatility(&cte.statement))
                .chain(update.assignments.iter().map(|a| a.value.volatility))
                .chain(update.from.iter().map(relation_volatility))
                .chain(update.predicate.iter().map(|e| e.volatility))
                .chain(update.returning.iter().map(|p| p.expression.volatility)),
        ),
        TypedStatementKind::Delete(delete) => max_volatility(
            delete
                .ctes
                .iter()
                .map(|cte| statement_volatility(&cte.statement))
                .chain(delete.using_relations.iter().map(relation_volatility))
                .chain(delete.predicate.iter().map(|e| e.volatility))
                .chain(delete.returning.iter().map(|p| p.expression.volatility)),
        ),
    }
}

fn insert_source_volatility(source: &TypedInsertSource) -> std::vec::IntoIter<Volatility> {
    let values = match source {
        TypedInsertSource::Values(values) => values
            .rows()
            .iter()
            .flat_map(|row| row.iter().map(|cell| cell.expression.volatility))
            .collect(),
        TypedInsertSource::Select(statement) => vec![statement_volatility(statement)],
        TypedInsertSource::DefaultValues => Vec::new(),
    };
    values.into_iter()
}

fn relation_volatility(relation: &TypedRelation) -> Volatility {
    match &relation.kind {
        TypedRelationKind::Table { .. } | TypedRelationKind::Cte { .. } => Volatility::Immutable,
        TypedRelationKind::Subquery(statement) => statement_volatility(statement),
        TypedRelationKind::Function { arguments, .. } => {
            max_volatility(arguments.iter().map(|argument| argument.volatility))
        }
        TypedRelationKind::Join {
            left,
            right,
            predicate,
            ..
        } => max_volatility(
            [relation_volatility(left), relation_volatility(right)]
                .into_iter()
                .chain(predicate.iter().map(|predicate| predicate.volatility)),
        ),
        TypedRelationKind::Values { rows } => max_volatility(
            rows.rows()
                .iter()
                .flat_map(|row| row.iter().map(|cell| cell.expression.volatility)),
        ),
        TypedRelationKind::SetOperation { left, right, .. } => {
            max_volatility([statement_volatility(left), statement_volatility(right)])
        }
    }
}
