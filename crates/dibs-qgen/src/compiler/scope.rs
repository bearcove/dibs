use dibs_pg_catalog::ColumnId;
use dibs_query_ir::{FieldId, RelationId, SourceOrigin};
use dibs_query_syntax::{SourceId, SourceSpan, Span};

use super::{CompileDiagnostic, CompileDiagnosticCode};

#[derive(Debug, Clone)]
pub(crate) struct RelationBinding {
    pub(crate) id: RelationId,
    pub(crate) columns: Vec<RelationColumnBinding>,
    pub(crate) origin: SourceOrigin,
    pub(crate) visible_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RelationColumnBinding {
    pub(crate) name: String,
    pub(crate) field: RelationFieldBinding,
}

#[derive(Debug, Clone)]
pub(crate) enum RelationFieldBinding {
    Catalog(ColumnId),
    Derived(FieldId),
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectionBinding {
    pub(crate) alias: String,
    pub(crate) expression: dibs_query_ir::HirExpression,
    pub(crate) origin: SourceOrigin,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct SelectScope {
    parent: Option<Box<SelectScope>>,
    relations: Vec<RelationBinding>,
    projections: Vec<ProjectionBinding>,
}

impl SelectScope {
    pub(crate) fn with_parent(parent: &SelectScope) -> Self {
        Self {
            parent: Some(Box::new(parent.clone())),
            relations: Vec::new(),
            projections: Vec::new(),
        }
    }

    pub(crate) fn insert_relation(
        &mut self,
        binding: RelationBinding,
    ) -> Result<(), CompileDiagnostic> {
        if let Some(existing) = self
            .relations
            .iter()
            .find(|existing| existing.visible_name == binding.visible_name)
        {
            return Err(CompileDiagnostic::new(
                CompileDiagnosticCode::DuplicateRelationBinding,
                binding.origin.span(),
                format!(
                    "relation binding '{}' is declared more than once",
                    binding.visible_name
                ),
            )
            .with_related(vec![existing.origin.span()]));
        }
        self.relations.push(binding);
        Ok(())
    }

    pub(crate) fn extend_relations(
        &mut self,
        bindings: impl IntoIterator<Item = RelationBinding>,
    ) -> Result<(), CompileDiagnostic> {
        for binding in bindings {
            self.insert_relation(binding)?;
        }
        Ok(())
    }

    pub(crate) fn insert_projection(
        &mut self,
        projection: ProjectionBinding,
    ) -> Result<(), CompileDiagnostic> {
        if let Some(existing) = self
            .projections
            .iter()
            .find(|existing| existing.alias == projection.alias)
        {
            return Err(CompileDiagnostic::new(
                CompileDiagnosticCode::DuplicateOutputLabel,
                projection.origin.span(),
                format!(
                    "projection label '{}' is declared more than once",
                    projection.alias
                ),
            )
            .with_related(vec![existing.origin.span()]));
        }
        self.projections.push(projection);
        Ok(())
    }

    pub(crate) fn projection(&self, name: &str) -> Option<&ProjectionBinding> {
        self.projections
            .iter()
            .find(|projection| projection.alias == name)
    }

    pub(crate) fn resolve_column(
        &self,
        source_id: SourceId,
        qualifier: Option<&str>,
        name: &str,
        span: Span,
    ) -> Result<(&RelationBinding, &RelationColumnBinding), CompileDiagnostic> {
        let primary = SourceSpan::new(source_id, span);
        if let Some(qualifier) = qualifier {
            if let Some(binding) = self
                .relations
                .iter()
                .find(|binding| binding.visible_name == qualifier)
            {
                let Some(column) = binding.columns.iter().find(|column| column.name == name) else {
                    return Err(CompileDiagnostic::new(
                        CompileDiagnosticCode::UnknownField,
                        primary,
                        format!("relation '{qualifier}' has no field '{name}'"),
                    ));
                };
                return Ok((binding, column));
            }
            if let Some(parent) = &self.parent {
                return parent.resolve_column(source_id, Some(qualifier), name, span);
            }
            return Err(CompileDiagnostic::new(
                CompileDiagnosticCode::UnknownRelation,
                primary,
                format!("unknown relation or alias '{qualifier}'"),
            ));
        }

        let mut matches = self.relations.iter().filter_map(|binding| {
            binding
                .columns
                .iter()
                .find(|column| column.name == name)
                .map(|column| (binding, column))
        });
        let Some(first) = matches.next() else {
            if let Some(parent) = &self.parent {
                return parent.resolve_column(source_id, None, name, span);
            }
            return Err(CompileDiagnostic::new(
                CompileDiagnosticCode::UnknownField,
                primary,
                format!("unknown field '{name}'"),
            ));
        };
        let competing = matches.collect::<Vec<_>>();
        if !competing.is_empty() {
            let mut related = vec![first.0.origin.span()];
            related.extend(competing.iter().map(|(binding, _)| binding.origin.span()));
            return Err(CompileDiagnostic::new(
                CompileDiagnosticCode::AmbiguousField,
                primary,
                format!("unqualified field '{name}' is ambiguous"),
            )
            .with_related(related));
        }
        Ok(first)
    }
}
