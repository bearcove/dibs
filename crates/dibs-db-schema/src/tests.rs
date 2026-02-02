use super::*;

#[test]
fn test_parse_fk_reference_dot_format() {
    assert_eq!(parse_fk_reference("users.id"), Some(("users", "id")));
    assert_eq!(parse_fk_reference("shop.id"), Some(("shop", "id")));
    assert_eq!(
        parse_fk_reference("category.parent_id"),
        Some(("category", "parent_id"))
    );
}

#[test]
fn test_parse_fk_reference_paren_format() {
    assert_eq!(parse_fk_reference("users(id)"), Some(("users", "id")));
    assert_eq!(parse_fk_reference("shop(id)"), Some(("shop", "id")));
    assert_eq!(
        parse_fk_reference("category(parent_id)"),
        Some(("category", "parent_id"))
    );
}

#[test]
fn test_parse_fk_reference_invalid() {
    assert_eq!(parse_fk_reference(""), None);
    assert_eq!(parse_fk_reference("users"), None);
    assert_eq!(parse_fk_reference(".id"), None);
    assert_eq!(parse_fk_reference("users."), None);
    assert_eq!(parse_fk_reference("(id)"), None);
    assert_eq!(parse_fk_reference("users("), None);
    assert_eq!(parse_fk_reference("users()"), None);
    assert_eq!(parse_fk_reference("()"), None);
}

#[test]
fn test_index_column_parse_simple() {
    let col = IndexColumn::parse("name");
    assert_eq!(col.name, "name");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::Default);
}

#[test]
fn test_index_column_parse_desc() {
    let col = IndexColumn::parse("created_at DESC");
    assert_eq!(col.name, "created_at");
    assert_eq!(col.order, SortOrder::Desc);
    assert_eq!(col.nulls, NullsOrder::Default);
}

#[test]
fn test_index_column_parse_asc() {
    let col = IndexColumn::parse("id ASC");
    assert_eq!(col.name, "id");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::Default);
}

#[test]
fn test_index_column_parse_nulls_first() {
    let col = IndexColumn::parse("reminder_sent_at NULLS FIRST");
    assert_eq!(col.name, "reminder_sent_at");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::First);
}

#[test]
fn test_index_column_parse_nulls_last() {
    let col = IndexColumn::parse("score NULLS LAST");
    assert_eq!(col.name, "score");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::Last);
}

#[test]
fn test_index_column_parse_desc_nulls_first() {
    let col = IndexColumn::parse("priority DESC NULLS FIRST");
    assert_eq!(col.name, "priority");
    assert_eq!(col.order, SortOrder::Desc);
    assert_eq!(col.nulls, NullsOrder::First);
}

#[test]
fn test_index_column_parse_desc_nulls_last() {
    let col = IndexColumn::parse("updated_at DESC NULLS LAST");
    assert_eq!(col.name, "updated_at");
    assert_eq!(col.order, SortOrder::Desc);
    assert_eq!(col.nulls, NullsOrder::Last);
}

#[test]
fn test_index_column_parse_asc_nulls_first() {
    let col = IndexColumn::parse("nullable_col ASC NULLS FIRST");
    assert_eq!(col.name, "nullable_col");
    assert_eq!(col.order, SortOrder::Asc);
    assert_eq!(col.nulls, NullsOrder::First);
}

#[test]
fn test_index_column_to_sql() {
    // Simple column
    let col = IndexColumn::new("name");
    assert_eq!(index_column_to_sql(&col), "\"name\"");

    // DESC
    let col = IndexColumn::desc("created_at");
    assert_eq!(index_column_to_sql(&col), "\"created_at\" DESC");

    // NULLS FIRST
    let col = IndexColumn::nulls_first("reminder_sent_at");
    assert_eq!(
        index_column_to_sql(&col),
        "\"reminder_sent_at\" NULLS FIRST"
    );

    // DESC NULLS LAST
    let col = IndexColumn {
        name: "priority".to_string(),
        order: SortOrder::Desc,
        nulls: NullsOrder::Last,
    };
    assert_eq!(index_column_to_sql(&col), "\"priority\" DESC NULLS LAST");
}
