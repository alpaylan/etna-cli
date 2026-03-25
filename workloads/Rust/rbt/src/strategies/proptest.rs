use crate::{implementation::Tree, spec, strategies::bespoke::insert};
use proptest::{collection::vec, prelude::*};

fn tree_strategy() -> BoxedStrategy<Tree> {
    vec((any::<i32>(), any::<i32>()), 0..33)
        .prop_map(|kvs| {
            kvs.into_iter()
                .fold(Tree::E, |tree, (key, value)| insert(key, value, tree))
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
        _ => return None,
    };

    Some(strategy)
}
