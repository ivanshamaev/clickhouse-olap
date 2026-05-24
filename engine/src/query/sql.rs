//! SQL builder: turns a QueryPlan + SubQuery into a parameterized SQL string.
//!
//! All user-supplied values travel as ClickHouse HTTP parameters ({pN:Type}).
//! Field names come exclusively from the semantic model — never from user input.

use super::plan::{QueryPlan, ResolvedField, SubQueryKind};

/// Build the SQL string for a specific sub-query inside the plan.
/// Returns `(sql, params)` where params are URL query params like `param_p0=LV`.
pub fn build_sql(plan: &QueryPlan, kind: &SubQueryKind) -> (String, Vec<(String, String)>) {
    let (row_fields, col_fields): (&[ResolvedField], &[ResolvedField]) = match kind {
        SubQueryKind::Main => (&plan.row_fields, &plan.col_fields),
        SubQueryKind::RowSubtotal { level } => (&plan.row_fields[..*level], &plan.col_fields),
        SubQueryKind::GrandTotal => (&[], &plan.col_fields),
    };

    let mut sql = String::with_capacity(512);

    // SELECT
    sql.push_str("SELECT\n");
    let mut select_parts: Vec<String> = Vec::new();

    for f in row_fields {
        select_parts.push(format!("    {} AS `{}`", f.sql_expr, f.alias));
    }
    for f in col_fields {
        select_parts.push(format!("    {} AS `{}`", f.sql_expr, f.alias));
    }
    for m in &plan.measures {
        select_parts.push(format!("    {} AS `{}`", m.agg.to_sql_expr(&m.field), m.id));
    }

    sql.push_str(&select_parts.join(",\n"));
    sql.push('\n');

    // FROM
    sql.push_str(&format!("FROM {}\n", plan.table));

    // WHERE
    if !plan.filter_params.clauses.is_empty() {
        sql.push_str("WHERE ");
        sql.push_str(&plan.filter_params.clauses.join("\n  AND "));
        sql.push('\n');
    }

    // GROUP BY
    let group_by_aliases: Vec<String> = row_fields
        .iter()
        .chain(col_fields.iter())
        .map(|f| format!("`{}`", f.alias))
        .collect();

    if !group_by_aliases.is_empty() {
        sql.push_str("GROUP BY ");
        sql.push_str(&group_by_aliases.join(", "));
        sql.push('\n');
    }

    // Limit pre-aggregate size (applied at ClickHouse level via LIMIT in GROUP BY)
    // Note: we add LIMIT n+1 so the executor can detect overflow.
    sql.push_str(&format!(
        "LIMIT {}\n",
        plan.max_groups + 1 // +1 to detect overflow
    ));

    // FORMAT — must be last
    sql.push_str("FORMAT JSONEachRow");

    // Params: convert to URL query params with "param_" prefix
    let url_params: Vec<(String, String)> = plan
        .filter_params
        .params
        .iter()
        .map(|(k, v)| (format!("param_{k}"), v.clone()))
        .collect();

    (sql, url_params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::AggType;
    use crate::query::plan::{FilterParams, QueryPlan, ResolvedField, SubQueryKind};
    use crate::query::ResolvedMeasure;

    fn make_plan_no_filters() -> QueryPlan {
        QueryPlan {
            table: "sales".to_string(),
            row_fields: vec![
                ResolvedField {
                    model_field: "region".to_string(),
                    sql_expr: "region".to_string(),
                    alias: "region".to_string(),
                },
                ResolvedField {
                    model_field: "product_category".to_string(),
                    sql_expr: "product_category".to_string(),
                    alias: "product_category".to_string(),
                },
            ],
            col_fields: vec![ResolvedField {
                model_field: "order_date".to_string(),
                sql_expr: "toStartOfMonth(order_date)".to_string(),
                alias: "order_date@month".to_string(),
            }],
            measures: vec![
                ResolvedMeasure {
                    id: "revenue".to_string(),
                    field: "amount".to_string(),
                    agg: AggType::Sum,
                    additive: true,
                    format: Some("currency".to_string()),
                },
                ResolvedMeasure {
                    id: "orders".to_string(),
                    field: "order_id".to_string(),
                    agg: AggType::CountDistinct,
                    additive: false,
                    format: Some("integer".to_string()),
                },
            ],
            filter_params: FilterParams::default(),
            sub_queries: vec![],
            max_groups: 200_000,
        }
    }

    #[test]
    fn main_query_golden_sql() {
        let plan = make_plan_no_filters();
        let (sql, params) = build_sql(&plan, &SubQueryKind::Main);

        assert!(params.is_empty(), "no filters → no params");
        assert!(sql.contains("SELECT"), "has SELECT");
        assert!(sql.contains("FROM sales"), "correct table");
        assert!(sql.contains("GROUP BY `region`, `product_category`, `order_date@month`"));
        assert!(sql.contains("sum(amount) AS `revenue`"));
        assert!(sql.contains("uniq(order_id) AS `orders`"));
        assert!(sql.contains("toStartOfMonth(order_date) AS `order_date@month`"));
        assert!(sql.ends_with("FORMAT JSONEachRow"));
    }

    #[test]
    fn row_subtotal_query_golden_sql() {
        let plan = make_plan_no_filters();
        let (sql, _) = build_sql(&plan, &SubQueryKind::RowSubtotal { level: 1 });

        // Only first row field (region) in GROUP BY, plus col field
        assert!(sql.contains("GROUP BY `region`, `order_date@month`"));
        // product_category must NOT be in GROUP BY
        assert!(!sql.contains("`product_category`"));
        // measures still present
        assert!(sql.contains("sum(amount)"));
    }

    #[test]
    fn grand_total_query_golden_sql() {
        let plan = make_plan_no_filters();
        let (sql, _) = build_sql(&plan, &SubQueryKind::GrandTotal);

        // No row dims in GROUP BY — only col dim
        assert!(sql.contains("GROUP BY `order_date@month`"));
        assert!(!sql.contains("`region`"));
        assert!(!sql.contains("`product_category`"));
    }

    #[test]
    fn grand_total_no_columns_has_no_group_by() {
        let mut plan = make_plan_no_filters();
        plan.col_fields.clear();
        let (sql, _) = build_sql(&plan, &SubQueryKind::GrandTotal);

        assert!(!sql.contains("GROUP BY"), "no GROUP BY when no col dims");
    }
}
