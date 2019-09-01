use diesel::{
    backend::Backend,
    expression::AsExpression,
    prelude::*,
    query_builder::{AstPass, QueryFragment},
    sql_types::*,
};

sql_function!(fn duplicate_document(id: Int4) -> Int4);

/// Create a SQL `COUNT(DISTINCT)` expression.
pub fn count_distinct<T, Expr>(expr: Expr)
-> CountDistinct<<Expr as AsExpression<T>>::Expression>
where
    Expr: AsExpression<T>,
{
    CountDistinct { expr: expr.as_expression() }
}

#[derive(Clone, Copy, Debug, QueryId, DieselNumericOps)]
pub struct CountDistinct<Expr> {
    expr: Expr,
}

impl<Expr> Expression for CountDistinct<Expr>
where
    Expr: Expression,
{
    type SqlType = Int8;
}

impl<Expr, DB> QueryFragment<DB> for CountDistinct<Expr>
where
    DB: Backend,
    Expr: QueryFragment<DB>,
{
    fn walk_ast(&self, mut out: AstPass<DB>) -> QueryResult<()> {
        out.push_sql("count(DISTINCT ");
        QueryFragment::walk_ast(&self.expr, out.reborrow())?;
        out.push_sql(")");
        Ok(())
    }
}

impl<Expr, QS> AppearsOnTable<QS> for CountDistinct<Expr>
where
    Expr: AppearsOnTable<QS>,
    Self: Expression,
{
}
