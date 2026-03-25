


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Tree {
    E,
    T(Box<Tree>, i32, i32, Box<Tree>),
}

impl Display for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tree::E => write!(f, "(E)"),
            Tree::T(l, k, v, r) => write!(f, "(T {} {} {} {})", l, k, v, r),
        }
    }
}

use std::fmt::Display;

use serde::{Deserialize, Serialize};
use Tree::*;

const FUEL: usize = 10000;

// Insert
pub(crate) fn insert(k: i32, v: i32, t: Tree) -> Tree {
    match t {
        E => T(Box::new(E), k, v, Box::new(E)),
        T(l, k2, v2, r) => {
            /* marauders:variation=insert;tags= */
            match () {
                _ if matches!(std::env::var("M_insert_1").as_deref(), Ok("active")) => {
                    T(Box::new(E), k, v, Box::new(E))
                },
                _ if matches!(std::env::var("M_insert_2").as_deref(), Ok("active")) => {
                    if k < k2 {
                        T(Box::new(insert(k, v, *l)), k2, v2, r)
                    } else {
                        T(l, k2, v, r)
                    }
                },
                _ if matches!(std::env::var("M_insert_3").as_deref(), Ok("active")) => {
                    if k < k2 {
                        T(Box::new(insert(k, v, *l)), k2, v2, r)
                    } else if k2 < k {
                        T(l, k2, v2, Box::new(insert(k, v, *r)))
                    } else {
                        T(l, k2, v2, r)
                    }
                },
                _ => {
                    if k < k2 {
                        T(Box::new(insert(k, v, *l)), k2, v2, r)
                    } else if k2 < k {
                        T(l, k2, v2, Box::new(insert(k, v, *r)))
                    } else {
                        T(l, k2, v, r)
                    }
                },
            }
        },
    }
}

// Join
pub(crate) fn join(l: Tree, r: Tree) -> Tree {
    match (l, r) {
        (E, r) => r,
        (l, E) => l,
        (T(l1, k1, v1, r1), T(l2, k2, v2, r2)) => {
            T(l1, k1, v1, Box::new(T(Box::new(join(*r1, *l2)), k2, v2, r2)))
        },
    }
}

// Delete
pub(crate) fn delete(k: i32, t: Tree) -> Tree {
    match t {
        E => E,
        T(l, k2, v2, r) => {
            /* marauders:variation=delete;tags= */
            match () {
                _ if matches!(std::env::var("M_delete_4").as_deref(), Ok("active")) => {
                    let _ = v2;
                    if k < k2 {
                        delete(k, *l)
                    } else if k2 < k {
                        delete(k, *r)
                    } else {
                        join(*l, *r)
                    }
                },
                _ if matches!(std::env::var("M_delete_5").as_deref(), Ok("active")) => {
                    if k2 < k {
                        T(Box::new(delete(k, *l)), k2, v2, r)
                    } else if k < k2 {
                        T(l, k2, v2, Box::new(delete(k, *r)))
                    } else {
                        join(*l, *r)
                    }
                },
                _ => {
                    if k < k2 {
                        T(Box::new(delete(k, *l)), k2, v2, r)
                    } else if k2 < k {
                        T(l, k2, v2, Box::new(delete(k, *r)))
                    } else {
                        join(*l, *r)
                    }
                },
            }
        },
    }
}

// Below
pub(crate) fn below(k: i32, t: Tree) -> Tree {
    match t {
        E => E,
        T(l, k2, v2, r) => {
            if k <= k2 {
                below(k, *l)
            } else {
                T(l, k2, v2, Box::new(below(k, *r)))
            }
        },
    }
}

// Above
pub(crate) fn above(k: i32, t: Tree) -> Tree {
    match t {
        E => E,
        T(l, k2, v2, r) => {
            if k2 <= k {
                above(k, *r)
            } else {
                T(Box::new(above(k, *l)), k2, v2, r)
            }
        },
    }
}

// Union with fuel
pub(crate) fn union_(l: Tree, r: Tree, f: usize) -> Tree {
    if f == 0 {
        return E;
    }
    let f1 = f - 1;
    match (l, r) {
        (E, r) => r,
        (l, E) => l,

        /* marauders:variation=union;tags= */
        (T(l1, k1, v1, r1), T(l2, k2, v2, r2)) if matches!(std::env::var("M_union_6").as_deref(), Ok("active")) => {
            T(l1, k1, v1, Box::new(T(Box::new(union_(*r1, *l2, f1)), k2, v2, r2)))
        },
        (T(l1, k1, v1, r1), T(l2, k2, v2, r2)) if matches!(std::env::var("M_union_7").as_deref(), Ok("active")) => {
            if k1 == k2 {
                T(
                    Box::new(union_(*l1, *l2, f1)),
                    k1,
                    v1,
                    Box::new(union_(*r1, *r2, f1)),
                )
            } else if k1 < k2 {
                T(
                    l1,
                    k1,
                    v1,
                    Box::new(T(Box::new(union_(*r1, *l2, f1)), k2, v2, r2)),
                )
            } else {
                union_(T(l2, k2, v2, r2), T(l1, k1, v1, r1), f1)
            }
        },
        (T(box l1, k1, v1, box r1), T(box l2, k2, v2, box r2)) if matches!(std::env::var("M_union_8").as_deref(), Ok("active")) => {
            if k1 == k2 {
                T(
                    Box::new(union_(l1, l2, f1)),
                    k1,
                    v1,
                    Box::new(union_(r1, r2, f1)),
                )
            } else if k1 < k2 {
                T(
                    Box::new(union_(l1, below(k1, l2.clone()), f1)),
                    k1,
                    v1,
                    Box::new(union_(r1, T(Box::new(above(k1, l2)), k2, v2, Box::new(r2)), f1)),
                )
            } else {
                union_(T(Box::new(l2), k2, v2, Box::new(r2)), T(Box::new(l1), k1, v1, Box::new(r1)), f1)
            }
        },
        (T(l1, k, v, r1), t) => {
            T(
                Box::new(union_(*l1, below(k, t.clone()), f1)),
                k,
                v,
                Box::new(union_(*r1, above(k, t), f1)),
            )
        },
    }
}

pub(crate) fn union(l: Tree, r: Tree) -> Tree {
    union_(l, r, FUEL)
}

// Find
pub(crate) fn find(k: i32, t: &Tree) -> Option<i32> {
    match t {
        E => None,
        T(l, k2, v2, r) => {
            if k < *k2 {
                find(k, l)
            } else if *k2 < k {
                find(k, r)
            } else {
                Some(*v2)
            }
        },
    }
}

// Size
pub(crate) fn size(t: &Tree) -> usize {
    match t {
        E => 0,
        T(l, _, _, r) => 1 + size(l) + size(r),
    }
}
