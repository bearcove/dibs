use dibs_pg_catalog::{
    CallableId, CatalogSnapshot, CollationId, ColumnId, ConstraintId, IndexId, OperatorId, TableId,
    TypeId,
};

/// Canonical SQL rendering fact for one stable catalog identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
#[repr(u8)]
pub enum CatalogRenderName {
    /// Qualified table identifier components.
    Table {
        /// Stable table identity.
        id: TableId,
        /// SQL identifier components in schema-to-object order.
        qualified_name: Vec<String>,
    },
    /// Unqualified column identifier.
    Column {
        /// Stable column identity.
        id: ColumnId,
        /// Exact column identifier.
        name: String,
    },
    /// Qualified callable identifier components.
    Callable {
        /// Stable callable identity.
        id: CallableId,
        /// SQL identifier components in schema-to-object order.
        qualified_name: Vec<String>,
    },
    /// Qualified operator identifier components.
    Operator {
        /// Stable operator identity.
        id: OperatorId,
        /// SQL operator name components in schema-to-operator order.
        qualified_name: Vec<String>,
    },
    /// Qualified type identifier components.
    Type {
        /// Stable type identity.
        id: TypeId,
        /// SQL identifier components in schema-to-object order.
        qualified_name: Vec<String>,
    },
    /// Qualified collation identifier components.
    Collation {
        /// Stable collation identity.
        id: CollationId,
        /// SQL identifier components in schema-to-object order.
        qualified_name: Vec<String>,
    },
    /// Unqualified constraint identifier.
    Constraint {
        /// Stable constraint identity.
        id: ConstraintId,
        /// Exact constraint identifier.
        name: String,
    },
    /// Unqualified index identifier.
    Index {
        /// Stable index identity.
        id: IndexId,
        /// Exact index identifier.
        name: String,
    },
}

impl CatalogRenderName {
    fn is_valid(&self) -> bool {
        match self {
            Self::Table { qualified_name, .. }
            | Self::Callable { qualified_name, .. }
            | Self::Operator { qualified_name, .. }
            | Self::Type { qualified_name, .. }
            | Self::Collation { qualified_name, .. } => valid_components(qualified_name),
            Self::Column { name, .. }
            | Self::Constraint { name, .. }
            | Self::Index { name, .. } => valid_component(name),
        }
    }
}

/// Canonical artifact-owned catalog rendering vocabulary used by SQL backends.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
#[facet(invariants = CatalogRenderNames::is_valid)]
pub struct CatalogRenderNames {
    entries: Vec<CatalogRenderName>,
}

impl CatalogRenderNames {
    /// Creates a canonical render vocabulary, rejecting invalid or duplicate identities.
    pub fn try_new(mut entries: Vec<CatalogRenderName>) -> Result<Self, CatalogRenderNameError> {
        entries.sort();
        if entries.iter().any(|entry| !entry.is_valid()) {
            return Err(CatalogRenderNameError::InvalidName);
        }
        if entries
            .windows(2)
            .any(|pair| same_identity(&pair[0], &pair[1]))
        {
            return Err(CatalogRenderNameError::DuplicateIdentity);
        }
        Ok(Self { entries })
    }

    /// Builds canonical render facts from one reviewed PostgreSQL 18 catalog snapshot.
    pub fn from_catalog(catalog: &CatalogSnapshot) -> Result<Self, CatalogRenderNameError> {
        let mut entries = Vec::new();
        for table in &catalog.tables {
            entries.push(CatalogRenderName::Table {
                id: table.id.clone(),
                qualified_name: split_qualified_identifier(&table.qualified_name)?,
            });
            for column in &table.columns {
                entries.push(CatalogRenderName::Column {
                    id: column.id.clone(),
                    name: column.name.clone(),
                });
            }
            entries.push(CatalogRenderName::Constraint {
                id: table.primary_key.id.clone(),
                name: constraint_name_from_id(table.primary_key.id.as_str())?,
            });
            for constraint in &table.unique_constraints {
                entries.push(CatalogRenderName::Constraint {
                    id: constraint.id.clone(),
                    name: constraint.name.clone(),
                });
            }
            for constraint in &table.foreign_keys {
                entries.push(CatalogRenderName::Constraint {
                    id: constraint.id.clone(),
                    name: constraint_name_from_id(constraint.id.as_str())?,
                });
            }
            for index in &table.indexes {
                entries.push(CatalogRenderName::Index {
                    id: index.id.clone(),
                    name: index.name.clone(),
                });
            }
        }
        for callable in &catalog.callables {
            entries.push(CatalogRenderName::Callable {
                id: callable.id.clone(),
                qualified_name: split_qualified_identifier(&callable.qualified_name)?,
            });
        }
        for operator in &catalog.operators {
            entries.push(CatalogRenderName::Operator {
                id: operator.id.clone(),
                qualified_name: split_qualified_identifier(&operator.qualified_name)?,
            });
        }
        for ty in &catalog.types {
            entries.push(CatalogRenderName::Type {
                id: ty.id.clone(),
                qualified_name: split_qualified_identifier(&ty.qualified_name)?,
            });
        }
        for collation in &catalog.collations {
            entries.push(CatalogRenderName::Collation {
                id: collation.clone(),
                qualified_name: collation_name_from_id(collation.as_str())?,
            });
        }
        Self::try_new(entries)
    }

    /// Returns canonically ordered entries.
    #[must_use]
    pub fn entries(&self) -> &[CatalogRenderName] {
        &self.entries
    }

    /// Returns the qualified table identifier components for an exact identity.
    #[must_use]
    pub fn table(&self, id: &TableId) -> Option<&[String]> {
        self.entries.iter().find_map(|entry| match entry {
            CatalogRenderName::Table {
                id: candidate,
                qualified_name,
            } if candidate == id => Some(qualified_name.as_slice()),
            _ => None,
        })
    }

    /// Returns the exact unqualified column identifier for an exact identity.
    #[must_use]
    pub fn column(&self, id: &ColumnId) -> Option<&str> {
        self.entries.iter().find_map(|entry| match entry {
            CatalogRenderName::Column {
                id: candidate,
                name,
            } if candidate == id => Some(name.as_str()),
            _ => None,
        })
    }

    /// Returns the qualified callable identifier components for an exact identity.
    #[must_use]
    pub fn callable(&self, id: &CallableId) -> Option<&[String]> {
        self.entries.iter().find_map(|entry| match entry {
            CatalogRenderName::Callable {
                id: candidate,
                qualified_name,
            } if candidate == id => Some(qualified_name.as_slice()),
            _ => None,
        })
    }

    /// Returns the qualified operator name components for an exact identity.
    #[must_use]
    pub fn operator(&self, id: &OperatorId) -> Option<&[String]> {
        self.entries.iter().find_map(|entry| match entry {
            CatalogRenderName::Operator {
                id: candidate,
                qualified_name,
            } if candidate == id => Some(qualified_name.as_slice()),
            _ => None,
        })
    }

    /// Returns the qualified type identifier components for an exact identity.
    #[must_use]
    pub fn type_name(&self, id: &TypeId) -> Option<&[String]> {
        self.entries.iter().find_map(|entry| match entry {
            CatalogRenderName::Type {
                id: candidate,
                qualified_name,
            } if candidate == id => Some(qualified_name.as_slice()),
            _ => None,
        })
    }

    /// Returns the qualified collation identifier components for an exact identity.
    #[must_use]
    pub fn collation(&self, id: &CollationId) -> Option<&[String]> {
        self.entries.iter().find_map(|entry| match entry {
            CatalogRenderName::Collation {
                id: candidate,
                qualified_name,
            } if candidate == id => Some(qualified_name.as_slice()),
            _ => None,
        })
    }

    /// Returns the exact unqualified constraint identifier for an exact identity.
    #[must_use]
    pub fn constraint(&self, id: &ConstraintId) -> Option<&str> {
        self.entries.iter().find_map(|entry| match entry {
            CatalogRenderName::Constraint {
                id: candidate,
                name,
            } if candidate == id => Some(name.as_str()),
            _ => None,
        })
    }

    /// Returns the exact unqualified index identifier for an exact identity.
    #[must_use]
    pub fn index(&self, id: &IndexId) -> Option<&str> {
        self.entries.iter().find_map(|entry| match entry {
            CatalogRenderName::Index {
                id: candidate,
                name,
            } if candidate == id => Some(name.as_str()),
            _ => None,
        })
    }

    fn is_valid(&self) -> bool {
        Self::try_new(self.entries.clone()).is_ok_and(|canonical| canonical.entries == self.entries)
    }
}

/// Invalid catalog render vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogRenderNameError {
    /// One identifier or qualified-name component was empty or contained NUL.
    InvalidName,
    /// One stable identity appeared more than once.
    DuplicateIdentity,
    /// A reviewed catalog record lacked a lossless SQL name representation.
    InvalidCatalogName,
}

impl std::fmt::Display for CatalogRenderNameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName => {
                formatter.write_str("catalog render name is empty or contains NUL")
            }
            Self::DuplicateIdentity => {
                formatter.write_str("catalog render vocabulary repeats a stable identity")
            }
            Self::InvalidCatalogName => {
                formatter.write_str("catalog record lacks a lossless SQL render name")
            }
        }
    }
}

impl std::error::Error for CatalogRenderNameError {}

fn valid_components(components: &[String]) -> bool {
    !components.is_empty()
        && components
            .iter()
            .all(|component| valid_component(component))
}

fn valid_component(component: &str) -> bool {
    !component.is_empty() && !component.contains('\0')
}

fn same_identity(left: &CatalogRenderName, right: &CatalogRenderName) -> bool {
    match (left, right) {
        (CatalogRenderName::Table { id: left, .. }, CatalogRenderName::Table { id: right, .. }) => {
            left == right
        }
        (
            CatalogRenderName::Column { id: left, .. },
            CatalogRenderName::Column { id: right, .. },
        ) => left == right,
        (
            CatalogRenderName::Callable { id: left, .. },
            CatalogRenderName::Callable { id: right, .. },
        ) => left == right,
        (
            CatalogRenderName::Operator { id: left, .. },
            CatalogRenderName::Operator { id: right, .. },
        ) => left == right,
        (CatalogRenderName::Type { id: left, .. }, CatalogRenderName::Type { id: right, .. }) => {
            left == right
        }
        (
            CatalogRenderName::Collation { id: left, .. },
            CatalogRenderName::Collation { id: right, .. },
        ) => left == right,
        (
            CatalogRenderName::Constraint { id: left, .. },
            CatalogRenderName::Constraint { id: right, .. },
        ) => left == right,
        (CatalogRenderName::Index { id: left, .. }, CatalogRenderName::Index { id: right, .. }) => {
            left == right
        }
        _ => false,
    }
}

fn split_qualified_identifier(value: &str) -> Result<Vec<String>, CatalogRenderNameError> {
    let components: Vec<_> = value.split('.').map(str::to_string).collect();
    valid_components(&components)
        .then_some(components)
        .ok_or(CatalogRenderNameError::InvalidCatalogName)
}

fn constraint_name_from_id(value: &str) -> Result<String, CatalogRenderNameError> {
    value
        .rsplit(':')
        .next()
        .filter(|name| valid_component(name))
        .map(str::to_string)
        .ok_or(CatalogRenderNameError::InvalidCatalogName)
}

fn collation_name_from_id(value: &str) -> Result<Vec<String>, CatalogRenderNameError> {
    let (_, name) = value
        .split_once(":collation:")
        .ok_or(CatalogRenderNameError::InvalidCatalogName)?;
    split_qualified_identifier(name)
}
