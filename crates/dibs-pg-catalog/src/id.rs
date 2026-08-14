use std::fmt;

macro_rules! string_id {
    ($name:ident, $docs:literal) => {
        #[doc = $docs]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, facet::Facet)]
        #[repr(transparent)]
        pub struct $name(String);

        impl $name {
            /// Creates an identity from its stable textual representation.
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// Returns the stable textual representation.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }
    };
}

string_id!(TableId, "Stable logical application table identity.");
string_id!(ColumnId, "Stable logical application column identity.");
string_id!(
    ConstraintId,
    "Stable logical application constraint identity."
);
string_id!(IndexId, "Stable logical application index identity.");
string_id!(TypeId, "Stable logical PostgreSQL type identity.");
string_id!(CallableId, "Stable logical function identity.");
string_id!(OperatorId, "Stable logical operator identity.");
string_id!(CastId, "Stable logical cast identity.");
string_id!(
    IoCoercionId,
    "Stable logical PostgreSQL explicit input/output coercion identity."
);
string_id!(CollationId, "Stable logical collation identity.");
string_id!(PgCodecId, "PostgreSQL storage codec identity.");
string_id!(WireCodecId, "Client wire codec identity.");
string_id!(ApiTypeId, "Generated language API type identity.");
