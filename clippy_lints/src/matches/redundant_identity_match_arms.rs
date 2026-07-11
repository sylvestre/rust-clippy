use super::REDUNDANT_IDENTITY_MATCH_ARMS;
use super::match_same_arms::pats_have_overlapping_values;
use super::needless_match::pat_same_as_expr;
use clippy_utils::diagnostics::span_lint_and_then;
use clippy_utils::ty::same_type_modulo_regions;
use clippy_utils::{peel_blocks, span_contains_comment};
use rustc_errors::Applicability;
use rustc_hir::{Arm, Expr, Pat, PatKind};
use rustc_lint::LateContext;
use rustc_middle::ty::TypeVisitableExt;
use rustc_span::Span;

pub(super) fn check<'tcx>(cx: &LateContext<'tcx>, ex: &Expr<'_>, arms: &'tcx [Arm<'_>], expr: &Expr<'_>) {
    // The merged catch-all arm binds and returns the scrutinee itself, so the match must
    // produce exactly the scrutinee's type. This also rules out matches relying on a
    // coercion inside the rebuilt arm bodies (e.g. `Err(e) => Err(e)` with `&mut T -> &T`).
    let results = cx.typeck_results();
    if !same_type_modulo_regions(results.expr_ty(ex), results.expr_ty(expr)) {
        return;
    }

    let is_identity = |arm: &Arm<'_>| arm.guard.is_none() && pat_same_as_expr(arm.pat, peel_blocks(arm.body));

    // The final arm must itself be an identity arm so the catch-all can take its place.
    // Arm attributes are left alone entirely: the rewrite could delete one or silently
    // change which arm it applies to.
    let [rest @ .., last] = arms else { return };
    if rest.is_empty()
        || !is_identity(last)
        || arms
            .iter()
            .any(|arm| arm.span.from_expansion() || !cx.tcx.hir_attrs(arm.hir_id).is_empty())
    {
        return;
    }

    // `res => res` returns the scrutinee at its own type. A final arm that already binds
    // the whole value imposes that exact constraint, but a final arm that rebuilds the
    // value can change lifetimes variant-by-variant (e.g. `Item<'a>` to `Item<'static>`
    // when the identity arms only cover lifetime-free variants), and typeck results have
    // regions erased, so require a region-free scrutinee type in that case.
    if !matches!(last.pat.kind, PatKind::Binding(..))
        && (results.expr_ty(ex).has_erased_regions() || results.expr_ty(ex).has_free_regions())
    {
        return;
    }

    let removable: Vec<usize> = (0..rest.len()).filter(|&i| is_identity(&rest[i])).collect();
    // Without a remaining non-identity arm the whole match is redundant, which is
    // `needless_match` territory.
    if removable.is_empty() || removable.len() == rest.len() {
        return;
    }

    // Merging moves each identity arm to the end. That must not hand its values to an
    // arm it previously shadowed: its pattern has to be disjoint from the pattern of
    // every non-identity arm that follows it (guards included, as they may side-effect).
    let rest_pats: Vec<&Pat<'_>> = rest.iter().map(|arm| arm.pat).collect();
    if pats_have_overlapping_values(cx, &rest_pats, |i, j| removable.contains(&i) && !removable.contains(&j)) {
        return;
    }

    let (name, last_needs_replacement) = match last.pat.kind {
        // `res => res` is already the merged form; only the earlier identity arms go.
        PatKind::Binding(_, _, ident, None) => (ident.to_string(), false),
        PatKind::Binding(_, _, ident, Some(_)) => (ident.to_string(), true),
        _ => (String::from("res"), true),
    };

    let spans: Vec<Span> = removable.iter().map(|&i| rest[i].span).chain([last.span]).collect();
    span_lint_and_then(
        cx,
        REDUNDANT_IDENTITY_MATCH_ARMS,
        spans,
        "these arms return the matched value unchanged",
        |diag| {
            let mut sugg: Vec<(Span, String)> = removable
                .iter()
                .map(|&i| (arms[i].span.until(arms[i + 1].span), String::new()))
                .collect();
            if last_needs_replacement {
                sugg.push((last.span, format!("{name} => {name}")));
            }
            // Applying the suggestion would delete any comment sitting in the replaced spans.
            let applicability = if sugg.iter().any(|&(span, _)| span_contains_comment(cx, span)) {
                Applicability::MaybeIncorrect
            } else {
                Applicability::MachineApplicable
            };
            diag.multipart_suggestion("merge them into a single catch-all arm", sugg, applicability);
        },
    );
}
