/// One PostgreSQL array axis with its exact length and lower bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PgArrayDimension {
    /// Number of positions on this axis.
    pub length: usize,
    /// PostgreSQL lower bound for this axis, commonly `1` but not fixed.
    pub lower_bound: i32,
}

/// Lossless PostgreSQL array value in row-major flat element order.
///
/// PostgreSQL arrays can have arbitrary rank and lower bounds, and individual
/// elements can be SQL `NULL`. A plain `Vec<T>` cannot represent those facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgArray<T> {
    elements: Vec<Option<T>>,
    dimensions: Vec<PgArrayDimension>,
}

impl<T> PgArray<T> {
    /// Constructs an array after validating the shape against the element count.
    pub fn try_new(
        elements: Vec<Option<T>>,
        dimensions: Vec<PgArrayDimension>,
    ) -> Result<Self, PgArrayError> {
        for dimension in &dimensions {
            if dimension.length > i32::MAX as usize {
                return Err(PgArrayError::DimensionLengthOutOfRange {
                    length: dimension.length,
                });
            }
        }
        let expected = if dimensions.is_empty() {
            0
        } else {
            dimensions
                .iter()
                .try_fold(1_usize, |product, dimension| {
                    product.checked_mul(dimension.length)
                })
                .ok_or(PgArrayError::ElementCountOverflow)?
        };
        if expected != elements.len() {
            return Err(PgArrayError::ElementCountMismatch {
                expected,
                actual: elements.len(),
            });
        }
        Ok(Self {
            elements,
            dimensions,
        })
    }

    /// Returns the runtime array rank.
    #[must_use]
    pub fn rank(&self) -> usize {
        self.dimensions.len()
    }

    /// Returns every axis in PostgreSQL order.
    #[must_use]
    pub fn dimensions(&self) -> &[PgArrayDimension] {
        &self.dimensions
    }

    /// Returns flattened row-major elements, retaining SQL-null elements.
    #[must_use]
    pub fn elements(&self) -> &[Option<T>] {
        &self.elements
    }

    /// Consumes the value into flattened elements and shape metadata.
    #[must_use]
    pub fn into_parts(self) -> (Vec<Option<T>>, Vec<PgArrayDimension>) {
        (self.elements, self.dimensions)
    }
}

/// Invalid PostgreSQL array value shape.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PgArrayError {
    /// Dimension lengths overflowed addressable element count.
    #[error("PostgreSQL array dimension product overflows usize")]
    ElementCountOverflow,
    /// A dimension length cannot be encoded in PostgreSQL's signed int32 field.
    #[error("PostgreSQL array dimension length {length} exceeds int32::MAX")]
    DimensionLengthOutOfRange {
        /// Supplied dimension length.
        length: usize,
    },
    /// Flattened element count did not match the dimension product.
    #[error("PostgreSQL array shape requires {expected} elements, got {actual}")]
    ElementCountMismatch {
        /// Product of all dimension lengths.
        expected: usize,
        /// Supplied flattened element count.
        actual: usize,
    },
}
