//! Upstream SID bridge: consume byte-SID machine rows from the pinned
//! my-lisp core and rewrite the real decoder closure so that every admitted
//! named callable is addressed by its Lisp-owned semantic ID, never by a
//! human-facing spelling.
//!
//! # Авторитет (Authority)
//!
//! Згідно з контрактом CML: `my-lisp` володіє семантикою мови та SID-таблицею
//! (`lib/surface/semantic-registry.lisp`). CML не винаходить власних імен
//! функцій: для registry-admitted поверхонь (`list`, `reverse`, `not`, ...)
//! ідентичність у IR — це саме бітовий ключ `Sid8`, отриманий через
//! `my_lisp::semantic_registry_export::semantic_id_for_admitted_surface`.
//! Локальні/модульні функції без registry-запису (напр. `reverse-onto`,
//! `utf8-continuation-byte?`) лишаються під своїми спейлінгами — вони не є
//! мовними семантичними ідентичностями.

use crate::ast::Expr as CExpr;
use my_lisp::Sid8;
use my_lisp::semantic_registry_export::semantic_id_bits;

/// Convert one raw my-lisp S-expression into a CML `Expr`.
///
/// `Sid` leaves remain typed exact `Sid8` values. They are never converted
/// to surface names, strings, symbols, integers, or other aliases; symbols
/// survive verbatim; numbers/rationals/strings pass through; lists recurse.
pub fn convert_lisp_expr(expr: &my_lisp::Expr) -> Result<CExpr, BridgeError> {
    use my_lisp::ExprKind;
    match &expr.kind {
        ExprKind::Sid(sid) => Ok(CExpr::Sid(*sid)),
        ExprKind::Symbol(s) => Ok(CExpr::Symbol(s.to_string())),
        ExprKind::Number(n, _) => Ok(CExpr::Integer(*n as i64)),
        ExprKind::String(s) => Ok(CExpr::String(s.to_string())),
        ExprKind::List(items) => items
            .iter()
            .map(convert_lisp_expr)
            .collect::<Result<Vec<_>, _>>()
            .map(CExpr::List),
        ExprKind::Pair(a, b) => Ok(CExpr::DottedList(
            vec![convert_lisp_expr(a)?],
            Box::new(convert_lisp_expr(b)?),
        )),
        other => Err(BridgeError::UnsupportedAst(format!("{other:?}"))),
    }
}

/// Render a `Sid8` as its canonical 8-bit ASCII address string.
pub fn address_of(sid: Sid8) -> String {
    semantic_id_bits(sid)
}

/// Resolve a `Sid8` to its Lisp-owned canonical surface spelling: the first
/// `en` (or fallback first) admitted surface row. This is registry-owned
/// identity, never a CML hardcoded table.
pub fn surface_of(sid: Sid8) -> String {
    use my_lisp::semantic_registry_export::admitted_surfaces_for_semantic_id;
    let rows = admitted_surfaces_for_semantic_id(sid);
    rows.iter()
        .find(|r| r.namespace == "en")
        .or_else(|| rows.first())
        .map(|r| r.name.to_string())
        .unwrap_or_else(|| address_of(sid))
}

/// Rewrites a decoded top-level byte-SID `define` row `(define name ...)`
/// so that the definition is keyed by `name`'s Lisp-owned semantic ID when
/// the spelling is registry-admitted, leaving `name` unchanged otherwise.
///
/// Input must already be a CML list whose head symbol is `define` (as
/// produced by `convert_lisp_expr` on a `(00001001 ...)` machine row). The
/// returned form keeps the same shape; only the definition name changes.
pub fn key_definition_by_sid(mut expr: CExpr) -> CExpr {
    if let CExpr::List(items) = &mut expr {
        if matches!(
            items.first(),
            Some(CExpr::Symbol(s)) if s.eq_ignore_ascii_case("define")
                || s.eq_ignore_ascii_case("def")
        ) {
            if let Some(CExpr::Symbol(name)) = items.get_mut(1) {
                if let Some(sid) =
                    my_lisp::semantic_registry_export::semantic_id_for_admitted_surface(name)
                {
                    *name = address_of(sid);
                }
            }
        }
    }
    expr
}

/// Bridge conversion error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeError {
    /// A my-lisp AST node without a CML projection (e.g. rationals).
    UnsupportedAst(String),
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedAst(msg) => write!(f, "unsupported my-lisp AST node: {msg}"),
        }
    }
}

impl std::error::Error for BridgeError {}

/// Rewrites all function call sites (symbols in call position) that are
/// registry-admitted to their Lisp-owned SID keys. Non-admitted symbols
/// (local functions, special forms, variables) are left unchanged.
pub fn rewrite_calls_to_sid(expr: &mut CExpr) {
    use my_lisp::semantic_registry_export::semantic_id_for_admitted_surface;

    fn walk(expr: &mut CExpr) {
        match expr {
            CExpr::Symbol(name) => {
                if let Some(sid) = semantic_id_for_admitted_surface(name) {
                    *name = address_of(sid);
                }
            }
            CExpr::List(items) => {
                for item in items.iter_mut() {
                    walk(item);
                }
            }
            CExpr::DottedList(items, tail) => {
                for item in items.iter_mut() {
                    walk(item);
                }
                walk(tail);
            }
            _ => {}
        }
    }
    walk(expr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use my_lisp::parse;

    #[test]
    fn byte_sid_define_row_converts_and_keeps_registry_sid_key() {
        let row = "(00001001 list (00001000 args args))";
        let exprs = parse(row).expect("my-lisp must parse the pinned byte-SID row");
        let mut forms: Vec<CExpr> = exprs
            .iter()
            .map(convert_lisp_expr)
            .collect::<Result<_, _>>()
            .expect("byte-SID row must project to CML");
        assert_eq!(forms.len(), 1);
        let form = key_definition_by_sid(forms.remove(0));
        let CExpr::List(items) = form else {
            panic!("expected a list form");
        };
        assert_eq!(
            items[0],
            CExpr::Symbol("define".to_string()),
            "define SID must project to its canonical surface"
        );
        assert_eq!(
            items[1],
            CExpr::Symbol("00100111".to_string()),
            "registry-admitted name `list` must be addressed by its Lisp-owned SID"
        );
    }

    #[test]
    fn non_registry_definition_keeps_local_spelling() {
        let row = "(00001001 reverse-onto (00001000 (values acc) (00000111)))";
        let exprs = parse(row).expect("my-lisp must parse the pinned byte-SID row");
        let mut forms: Vec<CExpr> = exprs
            .iter()
            .map(convert_lisp_expr)
            .collect::<Result<_, _>>()
            .expect("row must project");
        let form = key_definition_by_sid(forms.remove(0));
        let CExpr::List(items) = form else {
            panic!("expected a list form");
        };
        assert_eq!(
            items[1],
            CExpr::Symbol("reverse-onto".to_string()),
            "non-registry local function must keep its spelling"
        );
    }
}
