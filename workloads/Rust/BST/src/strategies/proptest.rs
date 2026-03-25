use crate::{implementation::Tree, spec, strategies::bespoke::insert_};
use proptest::{collection::vec, prelude::*};

fn tree_strategy() -> BoxedStrategy<Tree> {
    vec((any::<i32>(), any::<i32>()), 0..33)
        .prop_map(|kvs| {
            kvs.into_iter()
                .fold(Tree::E, |tree, (key, value)| insert_(key, value, tree))
        })
        .boxed()
}

pub fn strategy_for(property: &str) -> Option<BoxedStrategy<(String, Option<bool>)>> {
    let strategy = match property {
        "InsertValid" => (tree_strategy(), any::<i32>(), any::<i32>())
            .prop_map(|(t, k, v)| {
                (
                    format!("({} {} {})", t, k, v),
                    spec::prop_insert_valid(t, k, v),
                )
            })
            .boxed(),
        "DeleteValid" => (tree_strategy(), any::<i32>())
            .prop_map(|(t, k)| (format!("({} {})", t, k), spec::prop_delete_valid(t, k)))
            .boxed(),
        "UnionValid" => (tree_strategy(), tree_strategy())
            .prop_map(|(t1, t2)| (format!("({} {})", t1, t2), spec::prop_union_valid(t1, t2)))
            .boxed(),
        "InsertPost" => (tree_strategy(), any::<i32>(), any::<i32>(), any::<i32>())
            .prop_map(|(t, k, v, qk)| {
                (
                    format!("({} {} {} {})", t, k, v, qk),
                    spec::prop_insert_post(t, k, v, qk),
                )
            })
            .boxed(),
        "DeletePost" => (tree_strategy(), any::<i32>(), any::<i32>())
            .prop_map(|(t, k, qk)| {
                (
                    format!("({} {} {})", t, k, qk),
                    spec::prop_delete_post(t, k, qk),
                )
            })
            .boxed(),
        "UnionPost" => (tree_strategy(), tree_strategy(), any::<i32>())
            .prop_map(|(t1, t2, k)| {
                (
                    format!("({} {} {})", t1, t2, k),
                    spec::prop_union_post(t1, t2, k),
                )
            })
            .boxed(),
        "InsertModel" => (tree_strategy(), any::<i32>(), any::<i32>())
            .prop_map(|(t, k, v)| {
                (
                    format!("({} {} {})", t, k, v),
                    spec::prop_insert_model(t, k, v),
                )
            })
            .boxed(),
        "DeleteModel" => (tree_strategy(), any::<i32>())
            .prop_map(|(t, k)| (format!("({} {})", t, k), spec::prop_delete_model(t, k)))
            .boxed(),
        "UnionModel" => (tree_strategy(), tree_strategy())
            .prop_map(|(t1, t2)| (format!("({} {})", t1, t2), spec::prop_union_model(t1, t2)))
            .boxed(),
        "InsertInsert" => (
            tree_strategy(),
            any::<i32>(),
            any::<i32>(),
            any::<i32>(),
            any::<i32>(),
        )
            .prop_map(|(t, k, kp, v, vp)| {
                (
                    format!("({} {} {} {} {})", t, k, kp, v, vp),
                    spec::prop_insert_insert(t, k, kp, v, vp),
                )
            })
            .boxed(),
        "InsertDelete" => (tree_strategy(), any::<i32>(), any::<i32>(), any::<i32>())
            .prop_map(|(t, k, kp, v)| {
                (
                    format!("({} {} {} {})", t, k, kp, v),
                    spec::prop_insert_delete(t, k, kp, v),
                )
            })
            .boxed(),
        "InsertUnion" => (tree_strategy(), tree_strategy(), any::<i32>(), any::<i32>())
            .prop_map(|(t1, t2, k, v)| {
                (
                    format!("({} {} {} {})", t1, t2, k, v),
                    spec::prop_insert_union(t1, t2, k, v),
                )
            })
            .boxed(),
        "DeleteInsert" => (tree_strategy(), any::<i32>(), any::<i32>(), any::<i32>())
            .prop_map(|(t, k, kp, v)| {
                (
                    format!("({} {} {} {})", t, k, kp, v),
                    spec::prop_delete_insert(t, k, kp, v),
                )
            })
            .boxed(),
        "DeleteDelete" => (tree_strategy(), any::<i32>(), any::<i32>())
            .prop_map(|(t, k, kp)| {
                (
                    format!("({} {} {})", t, k, kp),
                    spec::prop_delete_delete(t, k, kp),
                )
            })
            .boxed(),
        "DeleteUnion" => (tree_strategy(), tree_strategy(), any::<i32>())
            .prop_map(|(t1, t2, k)| {
                (
                    format!("({} {} {})", t1, t2, k),
                    spec::prop_delete_union(t1, t2, k),
                )
            })
            .boxed(),
        "UnionDeleteInsert" => (tree_strategy(), tree_strategy(), any::<i32>(), any::<i32>())
            .prop_map(|(t1, t2, k, kp)| {
                (
                    format!("({} {} {} {})", t1, t2, k, kp),
                    spec::prop_union_delete_insert(t1, t2, k, kp),
                )
            })
            .boxed(),
        "UnionUnionIdempotent" => tree_strategy()
            .prop_map(|t| (format!("({})", t), spec::prop_union_union_idempotent(t)))
            .boxed(),
        "UnionUnionAssoc" => (tree_strategy(), tree_strategy(), tree_strategy())
            .prop_map(|(t1, t2, t3)| {
                (
                    format!("({} {} {})", t1, t2, t3),
                    spec::prop_union_union_assoc(t1, t2, t3),
                )
            })
            .boxed(),
        _ => return None,
    };

    Some(strategy)
}
