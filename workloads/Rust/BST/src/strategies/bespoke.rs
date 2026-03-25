use quickcheck::Arbitrary;

use crate::implementation::Tree;

use Tree::*;

pub(crate) fn insert_(k: i32, v: i32, t: Tree) -> Tree {
    match t {
        E => T(Box::new(E), k, v, Box::new(E)),
        T(l, k2, v2, r) => {
            /*| insert */
            if k < k2 {
                T(Box::new(insert_(k, v, *l)), k2, v2, r)
            } else if k2 < k {
                T(l, k2, v2, Box::new(insert_(k, v, *r)))
            } else {
                T(l, k2, v, r)
            }
        }
    }
}

impl Arbitrary for Tree {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let mut t = E;
        for _ in 0..g.size() {
            let k = i32::arbitrary(g);
            let v = i32::arbitrary(g);
            t = insert_(k, v, t);
        }
        t
    }
}
