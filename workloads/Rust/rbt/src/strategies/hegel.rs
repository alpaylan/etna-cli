use crate::{implementation::Tree, spec, strategies::bespoke::insert};
use hegel::{TestCase, composite, generators::integers};

fn draw_i32(tc: &TestCase) -> i32 {
    tc.draw(integers::<i32>())
}

#[composite]
fn usizes(tc: TestCase, min: usize, max: usize) -> usize {
    tc.draw(integers::<usize>().min_value(min).max_value(max))
}

fn draw_tree(tc: &TestCase) -> Tree {
    let mut tree = Tree::E;
    let size = tc.draw(usizes(0, 32));
    for _ in 0..size {
        let key = draw_i32(tc);
        let value = draw_i32(tc);
        tree = insert(key, value, tree);
    }
    tree
}

pub fn draw_case(property: &str, tc: &TestCase) -> Option<(String, Option<bool>)> {
    let out = match property {
        "InsertValid" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            let v = draw_i32(tc);
            (
                format!("({} {} {})", t, k, v),
                spec::prop_insert_valid(t, k, v),
            )
        }
        "DeleteValid" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            (format!("({} {})", t, k), spec::prop_delete_valid(t, k))
        }
        "InsertPost" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            let v = draw_i32(tc);
            let qk = draw_i32(tc);
            (
                format!("({} {} {} {})", t, k, v, qk),
                spec::prop_insert_post(t, k, v, qk),
            )
        }
        "DeletePost" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            let qk = draw_i32(tc);
            (
                format!("({} {} {})", t, k, qk),
                spec::prop_delete_post(t, k, qk),
            )
        }
        "InsertModel" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            let v = draw_i32(tc);
            (
                format!("({} {} {})", t, k, v),
                spec::prop_insert_model(t, k, v),
            )
        }
        "DeleteModel" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            (format!("({} {})", t, k), spec::prop_delete_model(t, k))
        }
        "InsertInsert" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            let kp = draw_i32(tc);
            let v = draw_i32(tc);
            let vp = draw_i32(tc);
            (
                format!("({} {} {} {} {})", t, k, kp, v, vp),
                spec::prop_insert_insert(t, k, kp, v, vp),
            )
        }
        "InsertDelete" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            let kp = draw_i32(tc);
            let v = draw_i32(tc);
            (
                format!("({} {} {} {})", t, k, kp, v),
                spec::prop_insert_delete(t, k, kp, v),
            )
        }
        "DeleteInsert" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            let kp = draw_i32(tc);
            let v = draw_i32(tc);
            (
                format!("({} {} {} {})", t, k, kp, v),
                spec::prop_delete_insert(t, k, kp, v),
            )
        }
        "DeleteDelete" => {
            let t = draw_tree(tc);
            let k = draw_i32(tc);
            let kp = draw_i32(tc);
            (
                format!("({} {} {})", t, k, kp),
                spec::prop_delete_delete(t, k, kp),
            )
        }
        _ => return None,
    };
    Some(out)
}
