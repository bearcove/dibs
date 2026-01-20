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
    /// Query parameters.
    pub params: Option<Params>,

    /// Source table to query from.
    pub from: String,

    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,

    /// Return only the first result.
    pub first: Option<bool>,

    /// Order by clause.
    pub order_by: Option<OrderBy>,

    /// Fields to select.
    pub select: Select,
}

/// ORDER BY clause.
#[derive(Debug, Facet)]
pub struct OrderBy {
    /// Column name -> direction ("asc" or "desc", None means asc)
    #[facet(flatten)]
    pub columns: HashMap<String, Option<String>>,
}

/// WHERE clause - filter conditions.
#[derive(Debug, Facet)]
pub struct Where {
    #[facet(flatten)]
    pub filters: HashMap<String, FilterValue>,
}

/// A filter value - can be a simple value or a tagged operator.
/// For now, just capture the raw string. We can expand later.
#[derive(Debug, Facet)]
#[facet(untagged)]
#[repr(u8)]
pub enum FilterValue {
    /// Parameter reference like $handle
    Param(String),
}

/// Query parameters.
#[derive(Debug, Facet)]
pub struct Params {
    #[facet(flatten)]
    pub params: HashMap<String, ParamType>,
}

/// Parameter type.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum ParamType {
    String,
    Int,
    Bool,
    Uuid,
    Decimal,
    Timestamp,
    /// Optional type: @optional(@string) -> Optional(vec![String])
    Optional(Vec<ParamType>),
}

/// SELECT clause.
#[derive(Debug, Facet)]
pub struct Select {
    #[facet(flatten)]
    pub fields: HashMap<String, Option<FieldDef>>,
}

/// A field definition - tagged values in select.
#[derive(Debug, Facet)]
#[facet(rename_all = "lowercase")]
#[repr(u8)]
pub enum FieldDef {
    /// A relation field (`@rel{...}`).
    Rel(Relation),
}

/// A relation definition (nested query on related table).
#[derive(Debug, Facet)]
pub struct Relation {
    /// Optional explicit table name.
    pub from: Option<String>,

    /// Filter conditions.
    #[facet(rename = "where")]
    pub where_clause: Option<Where>,

    /// Return only the first result.
    pub first: Option<bool>,

    /// Fields to select from the relation.
    pub select: Option<Select>,
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

    fn parse(source: &str) -> QueryFile {
        match facet_styx::from_str(source) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("{}", e.render("<test>", source));
                panic!("Parse failed");
            }
        }
    }

    #[test]
    fn test_parse_query_with_params() {
        let source = r#"
ProductByHandle @query{
    params{
        handle @string
        limit @int
    }
    from product
    select{ id, handle }
}
"#;
        let file: QueryFile = parse(source);
        let Decl::Query(q) = file.decls.get("ProductByHandle").unwrap();

        let params = q.params.as_ref().expect("should have params");
        assert_eq!(params.params.len(), 2);
        assert!(matches!(params.params.get("handle"), Some(ParamType::String)));
        assert!(matches!(params.params.get("limit"), Some(ParamType::Int)));
    }

    #[test]
    fn test_parse_query_with_optional_param() {
        let source = r#"
SearchProducts @query{
    params{
        query @string
        limit @optional(@int)
    }
    from product
    select{ id }
}
"#;
        let file: QueryFile = parse(source);
        let Decl::Query(q) = file.decls.get("SearchProducts").unwrap();

        let params = q.params.as_ref().expect("should have params");
        assert_eq!(params.params.len(), 2);
        assert!(matches!(params.params.get("query"), Some(ParamType::String)));

        // @optional(@int) should parse as Optional(vec![Int])
        match params.params.get("limit") {
            Some(ParamType::Optional(inner)) => {
                assert_eq!(inner.len(), 1);
                assert!(matches!(inner[0], ParamType::Int));
            }
            other => panic!("expected Optional, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_query_with_where() {
        let source = r#"
ProductByHandle @query{
    params{ handle @string }
    from product
    where{ handle $handle }
    select{ id, handle }
}
"#;
        let file: QueryFile = parse(source);
        let Decl::Query(q) = file.decls.get("ProductByHandle").unwrap();

        let where_clause = q.where_clause.as_ref().expect("should have where");
        assert_eq!(where_clause.filters.len(), 1);
        assert!(matches!(
            where_clause.filters.get("handle"),
            Some(FilterValue::Param(p)) if p == "$handle"
        ));
    }

    #[test]
    fn test_parse_query_with_first() {
        let source = r#"
SingleProduct @query{
    from product
    first true
    select{ id }
}
"#;
        let file: QueryFile = parse(source);
        let Decl::Query(q) = file.decls.get("SingleProduct").unwrap();

        assert_eq!(q.first, Some(true));
    }

    #[test]
    fn test_parse_query_with_order_by() {
        let source = r#"
ProductsSorted @query{
    from product
    order_by{ created_at desc, name }
    select{ id, name }
}
"#;
        let file: QueryFile = parse(source);
        let Decl::Query(q) = file.decls.get("ProductsSorted").unwrap();

        let order_by = q.order_by.as_ref().expect("should have order_by");
        assert_eq!(order_by.columns.len(), 2);
        assert_eq!(order_by.columns.get("created_at"), Some(&Some("desc".to_string())));
        assert_eq!(order_by.columns.get("name"), Some(&None)); // no direction = asc
    }

    #[test]
    fn test_parse_query_with_relation() {
        let source = r#"
ProductWithTranslation @query{
    params{ locale @string }
    from product
    select{
        id
        translation @rel{
            where{ locale $locale }
            first true
            select{ title, description }
        }
    }
}
"#;
        let file: QueryFile = parse(source);
        let Decl::Query(q) = file.decls.get("ProductWithTranslation").unwrap();

        assert_eq!(q.select.fields.len(), 2);

        // id is a simple column (None)
        assert!(q.select.fields.get("id").unwrap().is_none());

        // translation is a relation
        let translation = q.select.fields.get("translation").unwrap().as_ref().unwrap();
        match translation {
            FieldDef::Rel(rel) => {
                assert_eq!(rel.first, Some(true));
                let select = rel.select.as_ref().unwrap();
                assert_eq!(select.fields.len(), 2);
            }
        }
    }
}
