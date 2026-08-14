use super::*;

impl SemanticChecker<'_> {
    pub(super) fn is_integer(&self, type_id: &TypeId) -> bool {
        type_id == &self.types.smallint
            || type_id == &self.types.integer
            || type_id == &self.types.bigint
    }

    pub(super) fn is_numeric(&self, type_id: &TypeId) -> bool {
        self.type_facts(type_id)
            .is_some_and(|facts| facts.category == PgTypeCategory::Numeric)
    }

    pub(super) fn known_type(&self, type_id: &TypeId) -> Option<TypeId> {
        (type_id != &self.types.unknown).then(|| type_id.clone())
    }

    pub(super) fn select_pg_candidate<'a, T, I, F>(
        &self,
        candidates: I,
        actual: &[Option<TypeId>],
        expected_types: F,
    ) -> Result<ResolvedCandidate<&'a T>, SelectionError<&'a T>>
    where
        I: IntoIterator<Item = &'a T>,
        F: Fn(&'a T) -> Vec<&'a TypeId>,
    {
        let mut candidates = candidates
            .into_iter()
            .filter_map(|candidate| {
                let declared = expected_types(candidate);
                let resolved = self.resolve_polymorphic_arguments(&declared, actual)?;
                (resolved.argument_types.len() == actual.len()
                    && actual
                        .iter()
                        .zip(&resolved.argument_types)
                        .all(|(actual, expected)| {
                            actual
                                .as_ref()
                                .is_none_or(|actual| self.can_implicitly_coerce(actual, expected))
                        }))
                .then_some(ResolvedCandidate {
                    candidate,
                    argument_types: resolved.argument_types,
                })
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(SelectionError::None);
        }
        if let Some(index) = candidates.iter().position(|candidate| {
            actual
                .iter()
                .zip(&candidate.argument_types)
                .all(|(actual, expected)| actual.as_ref() == Some(expected))
        }) {
            return Ok(candidates.swap_remove(index));
        }
        if candidates.len() == 1 {
            return Ok(candidates.pop().expect("one candidate"));
        }

        let flattened_actual = actual
            .iter()
            .map(|actual| actual.as_ref().map(|actual| self.flatten_domain(actual)))
            .collect::<Vec<_>>();
        keep_max_by(&mut candidates, |candidate| {
            flattened_actual
                .iter()
                .zip(&candidate.argument_types)
                .filter(|(actual, expected)| actual.as_ref() == Some(expected))
                .count()
        });
        if candidates.len() == 1 {
            return Ok(candidates.pop().expect("one candidate"));
        }

        keep_max_by(&mut candidates, |candidate| {
            flattened_actual
                .iter()
                .zip(&candidate.argument_types)
                .filter(|(actual, expected)| {
                    actual.as_ref().is_some_and(|actual| {
                        actual != *expected
                            && self.type_facts(expected).is_some_and(|facts| {
                                facts.preferred && facts.category == self.type_category(actual)
                            })
                    })
                })
                .count()
        });
        if candidates.len() == 1 {
            return Ok(candidates.pop().expect("one candidate"));
        }

        for (index, actual) in flattened_actual.iter().enumerate() {
            if actual.is_some() {
                continue;
            }
            let categories = candidates
                .iter()
                .filter_map(|candidate| {
                    candidate
                        .argument_types
                        .get(index)
                        .and_then(|expected| self.type_facts(expected))
                        .map(|facts| facts.category)
                })
                .collect::<Vec<_>>();
            let category = if categories.contains(&PgTypeCategory::String) {
                PgTypeCategory::String
            } else if categories
                .first()
                .is_some_and(|first| categories.iter().all(|category| category == first))
            {
                categories[0]
            } else {
                return Err(SelectionError::Ambiguous(
                    candidates
                        .into_iter()
                        .map(|candidate| candidate.candidate)
                        .collect(),
                ));
            };
            candidates.retain(|candidate| {
                candidate
                    .argument_types
                    .get(index)
                    .and_then(|expected| self.type_facts(expected))
                    .is_some_and(|facts| facts.category == category)
            });
            let any_preferred = candidates.iter().any(|candidate| {
                candidate
                    .argument_types
                    .get(index)
                    .and_then(|expected| self.type_facts(expected))
                    .is_some_and(|facts| facts.preferred)
            });
            if any_preferred {
                candidates.retain(|candidate| {
                    candidate
                        .argument_types
                        .get(index)
                        .and_then(|expected| self.type_facts(expected))
                        .is_some_and(|facts| facts.preferred)
                });
            }
            if candidates.len() == 1 {
                return Ok(candidates.pop().expect("one candidate"));
            }
        }

        let known = flattened_actual
            .iter()
            .filter_map(Clone::clone)
            .collect::<Vec<_>>();
        if !known.is_empty() && known.iter().all(|actual| actual == &known[0]) {
            let assumed = &known[0];
            candidates.retain(|candidate| {
                candidate
                    .argument_types
                    .iter()
                    .enumerate()
                    .all(|(index, expected)| {
                        flattened_actual[index].is_some()
                            || self.can_implicitly_coerce(assumed, expected)
                    })
            });
            if candidates.len() == 1 {
                return Ok(candidates.pop().expect("one candidate"));
            }
        }
        Err(SelectionError::Ambiguous(
            candidates
                .into_iter()
                .map(|candidate| candidate.candidate)
                .collect(),
        ))
    }

    pub(super) fn coercion(
        &self,
        source: &TypedExpression,
        target: &TypeId,
        context: CoercionContext,
    ) -> Result<Option<dibs_query_ir::TypedCoercion>, CheckError> {
        if &source.type_id == target {
            return Ok(None);
        }
        let result_nullability = if source.nullability.is_nullable() {
            Nullability::nullable(NullabilityEvidence::CastPropagation)
        } else {
            Nullability::not_null(NullabilityEvidence::CastPropagation)
        };
        let evidence = if source.type_id == self.types.unknown {
            CoercionEvidence::UnknownLiteral {
                resolved: target.clone(),
            }
        } else if let Some(domain) = self
            .catalog
            .type_by_id(&source.type_id)
            .and_then(|ty| ty.domain.as_ref())
            && &domain.base_type == target
        {
            CoercionEvidence::DomainBase {
                domain: source.type_id.clone(),
                base: target.clone(),
            }
        } else {
            if let Some(path) = self.find_cast_path(&source.type_id, target, context) {
                CoercionEvidence::CatalogCastPath {
                    steps: path
                        .into_iter()
                        .map(|cast| TypedCastStep {
                            cast_id: cast.id.clone(),
                            source_type: cast.source.clone(),
                            target_type: cast.target.clone(),
                            context: catalog_cast_context(cast.context),
                        })
                        .collect(),
                }
            } else if context == CoercionContext::Explicit {
                let coercion = self
                    .catalog
                    .io_coercion(&source.type_id, target)
                    .ok_or_else(|| TypeResolutionError::IncompatibleCommonType {
                        types: vec![Some(source.type_id.clone()), Some(target.clone())],
                    })?;
                CoercionEvidence::ExplicitIo {
                    postgres_major: self.catalog.postgres_major,
                    coercion_id: coercion.id.clone(),
                    source: coercion.source.clone(),
                    target: coercion.target.clone(),
                }
            } else {
                return Err(TypeResolutionError::IncompatibleCommonType {
                    types: vec![Some(source.type_id.clone()), Some(target.clone())],
                }
                .into());
            }
        };
        Ok(Some(dibs_query_ir::TypedCoercion {
            source_type: source.type_id.clone(),
            target_type: target.clone(),
            target_typmod: None,
            result_nullability,
            evidence,
        }))
    }

    pub(super) fn common_type(&self, inputs: &[TypeId]) -> Result<TypeId, CheckError> {
        if inputs.is_empty() {
            return Err(self.incompatible_common_type(inputs));
        }
        if inputs[0] != self.types.unknown && inputs.iter().all(|input| input == &inputs[0]) {
            return Ok(inputs[0].clone());
        }
        let flattened = inputs
            .iter()
            .map(|input| self.flatten_domain(input))
            .collect::<Vec<_>>();
        let known = flattened
            .iter()
            .filter(|input| *input != &self.types.unknown)
            .cloned()
            .collect::<Vec<_>>();
        if known.is_empty() {
            return Ok(self.types.text.clone());
        }
        let category = self
            .type_facts(&known[0])
            .map(|facts| facts.category)
            .ok_or_else(|| self.incompatible_common_type(inputs))?;
        if known
            .iter()
            .any(|input| self.type_category(input) != category)
        {
            return Err(self.incompatible_common_type(inputs));
        }
        let mut candidate = known[0].clone();
        for input in &known[1..] {
            if self.type_is_preferred(&candidate) {
                break;
            }
            let candidate_to_input = self.can_implicitly_coerce(&candidate, input);
            let input_to_candidate = self.can_implicitly_coerce(input, &candidate);
            if candidate_to_input && !input_to_candidate {
                candidate = input.clone();
            }
        }
        if flattened.iter().all(|input| {
            input == &self.types.unknown || self.can_implicitly_coerce(input, &candidate)
        }) {
            Ok(candidate)
        } else {
            Err(self.incompatible_common_type(inputs))
        }
    }

    fn incompatible_common_type(&self, inputs: &[TypeId]) -> CheckError {
        TypeResolutionError::IncompatibleCommonType {
            types: inputs
                .iter()
                .map(|type_id| self.known_type(type_id))
                .collect(),
        }
        .into()
    }

    fn type_facts(&self, type_id: &TypeId) -> Option<&CatalogType> {
        self.catalog.type_by_id(type_id)
    }

    fn flatten_domain(&self, type_id: &TypeId) -> TypeId {
        let mut current = type_id.clone();
        while let Some(base) = self
            .type_facts(&current)
            .and_then(|facts| facts.domain.as_ref())
            .map(|domain| domain.base_type.clone())
        {
            current = base;
        }
        current
    }

    fn type_category(&self, type_id: &TypeId) -> PgTypeCategory {
        self.type_facts(type_id)
            .map_or(PgTypeCategory::UserDefined, |facts| facts.category)
    }

    fn type_is_preferred(&self, type_id: &TypeId) -> bool {
        self.type_facts(type_id)
            .is_some_and(|facts| facts.preferred)
    }

    pub(super) fn resolve_polymorphic_arguments(
        &self,
        declared_arguments: &[&TypeId],
        actual: &[Option<TypeId>],
    ) -> Option<PolymorphicResolution> {
        if declared_arguments.len() != actual.len() {
            return None;
        }
        let mut any_bindings = Vec::new();
        let mut exact_element = None;
        let mut exact_array = None;
        let mut compatible_inputs = Vec::new();
        for (declared, actual) in declared_arguments.iter().zip(actual) {
            let Some(family) = self
                .type_facts(declared)
                .and_then(|facts| facts.polymorphic)
            else {
                continue;
            };
            let actual = actual.as_ref().unwrap_or(&self.types.unknown);
            let actual_facts = self.type_facts(actual)?;
            match family {
                PolymorphicType::Any => {
                    any_bindings.push(actual.clone());
                }
                PolymorphicType::AnyElement | PolymorphicType::AnyNonArray => {
                    if family == PolymorphicType::AnyNonArray
                        && actual_facts.kind == PgTypeKind::Array
                    {
                        return None;
                    }
                    unify_exact(&mut exact_element, actual)?;
                }
                PolymorphicType::AnyEnum => {
                    if actual_facts.kind != PgTypeKind::Enum {
                        return None;
                    }
                    unify_exact(&mut exact_element, actual)?;
                }
                PolymorphicType::AnyArray => {
                    if actual_facts.kind != PgTypeKind::Array {
                        return None;
                    }
                    unify_exact(&mut exact_array, actual)?;
                    unify_exact(&mut exact_element, actual_facts.element_type.as_ref()?)?;
                }
                PolymorphicType::AnyCompatible | PolymorphicType::AnyCompatibleNonArray => {
                    if family == PolymorphicType::AnyCompatibleNonArray
                        && actual_facts.kind == PgTypeKind::Array
                    {
                        return None;
                    }
                    compatible_inputs.push(actual.clone());
                }
                PolymorphicType::AnyCompatibleArray => {
                    if actual_facts.kind != PgTypeKind::Array {
                        return None;
                    }
                    compatible_inputs.push(actual_facts.element_type.clone()?);
                }
            }
        }
        if let Some(element) = &exact_element {
            let array_for_element = self.array_type_for_element(element);
            if exact_array
                .as_ref()
                .is_some_and(|array| Some(array) != array_for_element.as_ref())
            {
                return None;
            }
            exact_array = exact_array.or(array_for_element);
        }
        let compatible_element = if compatible_inputs.is_empty() {
            None
        } else {
            self.common_type(&compatible_inputs).ok()
        };
        let compatible_array = compatible_element
            .as_ref()
            .and_then(|element| self.array_type_for_element(element));
        let mut any_index = 0;
        let mut argument_types = Vec::with_capacity(declared_arguments.len());
        for declared in declared_arguments {
            let Some(family) = self
                .type_facts(declared)
                .and_then(|facts| facts.polymorphic)
            else {
                argument_types.push((*declared).clone());
                continue;
            };
            let resolved = match family {
                PolymorphicType::Any => {
                    let resolved = any_bindings.get(any_index).cloned();
                    any_index += 1;
                    resolved
                }
                PolymorphicType::AnyElement
                | PolymorphicType::AnyNonArray
                | PolymorphicType::AnyEnum => exact_element.clone(),
                PolymorphicType::AnyArray => exact_array.clone(),
                PolymorphicType::AnyCompatible | PolymorphicType::AnyCompatibleNonArray => {
                    compatible_element.clone()
                }
                PolymorphicType::AnyCompatibleArray => compatible_array.clone(),
            }?;
            argument_types.push(resolved);
        }
        Some(PolymorphicResolution { argument_types })
    }

    pub(super) fn resolve_polymorphic_result(
        &self,
        declared_result: &TypeId,
        declared_arguments: &[TypeId],
        resolved_arguments: &[TypeId],
    ) -> Result<TypeId, CheckError> {
        let Some(result_family) = self
            .type_facts(declared_result)
            .and_then(|facts| facts.polymorphic)
        else {
            return Ok(declared_result.clone());
        };
        let mut any_result = None;
        let mut simple_element = None;
        let mut simple_array = None;
        let mut compatible_element = None;
        for (declared, resolved) in declared_arguments.iter().zip(resolved_arguments) {
            let Some(family) = self
                .type_facts(declared)
                .and_then(|facts| facts.polymorphic)
            else {
                continue;
            };
            match family {
                PolymorphicType::Any => {
                    any_result.get_or_insert_with(|| resolved.clone());
                }
                PolymorphicType::AnyElement
                | PolymorphicType::AnyNonArray
                | PolymorphicType::AnyEnum => simple_element = Some(resolved.clone()),
                PolymorphicType::AnyArray => {
                    simple_array = Some(resolved.clone());
                    simple_element = self
                        .type_facts(resolved)
                        .and_then(|facts| facts.element_type.clone());
                }
                PolymorphicType::AnyCompatible | PolymorphicType::AnyCompatibleNonArray => {
                    compatible_element = Some(resolved.clone());
                }
                PolymorphicType::AnyCompatibleArray => {
                    compatible_element = self
                        .type_facts(resolved)
                        .and_then(|facts| facts.element_type.clone());
                }
            }
        }
        let result = match result_family {
            PolymorphicType::Any => any_result,
            PolymorphicType::AnyElement
            | PolymorphicType::AnyNonArray
            | PolymorphicType::AnyEnum => simple_element,
            PolymorphicType::AnyArray => simple_array.or_else(|| {
                simple_element
                    .as_ref()
                    .and_then(|element| self.array_type_for_element(element))
            }),
            PolymorphicType::AnyCompatible | PolymorphicType::AnyCompatibleNonArray => {
                compatible_element
            }
            PolymorphicType::AnyCompatibleArray => compatible_element
                .as_ref()
                .and_then(|element| self.array_type_for_element(element)),
        };
        result.ok_or_else(|| {
            TypeResolutionError::MissingCatalogFact {
                kind: "polymorphic-result-binding",
                identity: declared_result.to_string(),
            }
            .into()
        })
    }

    fn array_type_for_element(&self, element: &TypeId) -> Option<TypeId> {
        self.catalog
            .types
            .iter()
            .find(|facts| facts.element_type.as_ref() == Some(element))
            .map(|facts| facts.id.clone())
    }

    fn can_implicitly_coerce(&self, source: &TypeId, target: &TypeId) -> bool {
        source == target
            || source == &self.types.unknown
            || self
                .catalog
                .type_by_id(source)
                .and_then(|ty| ty.domain.as_ref())
                .is_some_and(|domain| &domain.base_type == target)
            || self
                .catalog
                .cast_path(source, target, CastContext::Implicit)
                .is_some()
    }

    fn find_cast_path(
        &self,
        source: &TypeId,
        target: &TypeId,
        context: CoercionContext,
    ) -> Option<Vec<&CatalogCast>> {
        self.catalog
            .cast_path(source, target, catalog_coercion_context(context))
    }

    pub(super) fn check_values(
        &self,
        rows: &[Vec<HirExpression>],
        context: &CheckContext<'_>,
        expected: Option<&[TypeId]>,
    ) -> Result<TypedValues, CheckError> {
        let width = rows.first().map_or(0, Vec::len);
        let mut columns = Vec::with_capacity(width);
        for column in 0..width {
            if let Some(expected) = expected.and_then(|types| types.get(column)) {
                columns.push(expected.clone());
            } else {
                let initial = rows
                    .iter()
                    .map(|row| self.check_expression(&row[column], context, None))
                    .collect::<Result<Vec<_>, _>>()?;
                columns.push(
                    self.common_type(
                        &initial
                            .iter()
                            .map(|expression| expression.type_id.clone())
                            .collect::<Vec<_>>(),
                    )?,
                );
            }
        }
        let typed_rows = rows
            .iter()
            .map(|row| {
                row.iter()
                    .zip(&columns)
                    .map(|(expression, target)| {
                        self.check_argument(expression, context, target, CoercionContext::Implicit)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let column_contracts = columns
            .into_iter()
            .enumerate()
            .map(|(column, type_id)| {
                let nullable = typed_rows
                    .iter()
                    .any(|row| argument_output_nullability(&row[column]).is_nullable());
                TypedValuesColumn {
                    type_id: type_id.clone(),
                    typmod: None,
                    nullability: if nullable {
                        Nullability::nullable(NullabilityEvidence::ValuesPropagation)
                    } else {
                        Nullability::not_null(NullabilityEvidence::ValuesPropagation)
                    },
                    common_type: CoercionEvidence::CommonType {
                        resolved: type_id,
                        inputs: typed_rows
                            .iter()
                            .map(|row| row[column].expression.type_id.clone())
                            .collect(),
                    },
                }
            })
            .collect();
        Ok(TypedValues::try_new(typed_rows, column_contracts)?)
    }

    pub(super) fn coerce_set_outputs(
        &self,
        left: &mut TypedStatement,
        right: &mut TypedStatement,
    ) -> Result<(), CheckError> {
        let left_len = statement_projections(left).len();
        let right_len = statement_projections(right).len();
        if left_len != right_len {
            return Err(CheckError::SetColumnCountMismatch {
                left: left_len,
                right: right_len,
            });
        }
        let common = statement_projections(left)
            .iter()
            .zip(statement_projections(right))
            .map(|(left, right)| {
                self.common_type(&[
                    left.output_type_id().clone(),
                    right.output_type_id().clone(),
                ])
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.apply_projection_coercions(left, &common)?;
        self.apply_projection_coercions(right, &common)?;
        Ok(())
    }

    fn apply_projection_coercions(
        &self,
        statement: &mut TypedStatement,
        common: &[TypeId],
    ) -> Result<(), CheckError> {
        for (projection, target) in statement_projections_mut(statement).iter_mut().zip(common) {
            projection.coercion =
                self.coercion(&projection.expression, target, CoercionContext::Implicit)?;
        }
        Ok(())
    }

    pub(super) fn apply_assignment_projection_coercions(
        &self,
        statement: &mut TypedStatement,
        targets: &[TypeId],
    ) -> Result<(), CheckError> {
        for (projection, target) in statement_projections_mut(statement).iter_mut().zip(targets) {
            ensure_uncoerced_assignment_projection(projection)?;
            projection.coercion =
                self.coercion(&projection.expression, target, CoercionContext::Assignment)?;
        }
        Ok(())
    }
}

fn keep_max_by<T>(values: &mut Vec<T>, score: impl Fn(&T) -> usize) {
    let maximum = values.iter().map(&score).max().unwrap_or(0);
    values.retain(|value| score(value) == maximum);
}

fn unify_exact(binding: &mut Option<TypeId>, actual: &TypeId) -> Option<()> {
    match binding {
        Some(bound) if bound != actual => None,
        Some(_) => Some(()),
        None => {
            *binding = Some(actual.clone());
            Some(())
        }
    }
}

fn catalog_coercion_context(context: CoercionContext) -> CastContext {
    match context {
        CoercionContext::Implicit => CastContext::Implicit,
        CoercionContext::Assignment => CastContext::Assignment,
        CoercionContext::Explicit => CastContext::Explicit,
    }
}

fn catalog_cast_context(context: CastContext) -> CoercionContext {
    match context {
        CastContext::Implicit => CoercionContext::Implicit,
        CastContext::Assignment => CoercionContext::Assignment,
        CastContext::Explicit => CoercionContext::Explicit,
    }
}

fn argument_output_nullability(argument: &TypedArgument) -> &Nullability {
    argument
        .coercion
        .as_ref()
        .map_or(&argument.expression.nullability, |coercion| {
            &coercion.result_nullability
        })
}

fn ensure_uncoerced_assignment_projection(projection: &TypedProjection) -> Result<(), CheckError> {
    if projection.coercion.is_some() {
        return Err(CheckError::InvalidTypedShape(
            dibs_query_ir::TypedShapeError::Coercion,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_projection_guard_rejects_existing_coercion() {
        let integer = TypeId::new("pg18:type:base:pg_catalog.integer");
        let bigint = TypeId::new("pg18:type:base:pg_catalog.bigint");
        let projection = TypedProjection {
            field_id: FieldId::new(1),
            sql_label: "value".to_string(),
            expression: TypedExpression {
                id: ExpressionId::new(1),
                origin: SourceOrigin::generated(
                    dibs_query_ir::GeneratedOrigin::Structural,
                    Vec::new(),
                ),
                type_id: integer.clone(),
                typmod: None,
                nullability: Nullability::not_null(NullabilityEvidence::CastPropagation),
                volatility: Volatility::Immutable,
                kind: TypedExpressionKind::Parameter(ParameterId::new(1)),
            },
            coercion: Some(dibs_query_ir::TypedCoercion {
                source_type: integer.clone(),
                target_type: bigint.clone(),
                target_typmod: None,
                result_nullability: Nullability::not_null(NullabilityEvidence::CastPropagation),
                evidence: CoercionEvidence::CatalogCastPath {
                    steps: vec![TypedCastStep {
                        cast_id: dibs_pg_catalog::CastId::new("pg18:cast:integer->bigint"),
                        source_type: integer,
                        target_type: bigint,
                        context: CoercionContext::Implicit,
                    }],
                },
            }),
        };

        assert_eq!(
            ensure_uncoerced_assignment_projection(&projection),
            Err(CheckError::InvalidTypedShape(
                dibs_query_ir::TypedShapeError::Coercion
            ))
        );
    }
}
