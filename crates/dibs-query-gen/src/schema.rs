//! Query DSL schema types using Facet.
//!
//! These types are deserialized directly from styx using facet-styx.

use facet::Facet;
use std::collections::HashMap;

/// A query file - top level is a map of declaration names to declarations.
#[derive(Debug, Facet)]
pub struct QueryFile {
    #[facet(flatten)]
    pub decls: HashMap<String, Decl>,
}

/// A declaration in a query file.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum Decl {
    /// A query declaration.
    Query(Query),
}

/// A query definition.
#[derive(Debug, Facet)]
pub struct Query {
    /// Source table to query from.
    pub from: String,

    /// Fields to select.
    pub select: Select,
}

/// SELECT clause.
#[derive(Debug, Facet)]
pub struct Select {
    #[facet(flatten)]
    pub fields: HashMap<String, Option<FieldDef>>,
}

/// A field definition - None means simple column, Some means complex (relation, etc).
#[derive(Debug, Facet)]
pub struct FieldDef {
    // TODO: add relation support etc.
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_styx::RenderError;
    use facet_testhelpers::test;
    use tracing::debug;

    #[test]
    fn test_parse_minimal_query() {
        let source = r#"
AllProducts @query{
    from product
    select{ id, handle }
}
"#;
        debug!("Parsing source: {:?}", source);
        let file: QueryFile = match facet_styx::from_str(source) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("{}", e.render("<test>", source));
                panic!("Parse failed");
            }
        };
        debug!("Parsed file: {:?}", file);
        assert_eq!(file.decls.len(), 1);

        let (name, decl) = file.decls.iter().next().unwrap();
        assert_eq!(name, "AllProducts");

        match decl {
            Decl::Query(q) => {
                assert_eq!(q.from, "product");
                assert_eq!(q.select.fields.len(), 2);
            }
        }
    }
}
