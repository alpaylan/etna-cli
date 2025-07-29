use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Typ {
    TBool,
    TFun(Box<Typ>, Box<Typ>),
}

impl Display for Typ {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Typ::TBool => write!(f, "(TBool)"),
            Typ::TFun(param, ret) => write!(f, "(TFun {} {})", param, ret),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Var(i32),
    Bool(bool),
    Abs(Typ, Box<Expr>),
    App(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn size(&self) -> usize {
        match self {
            Expr::Var(_) => 1,
            Expr::Bool(_) => 1,
            Expr::Abs(_, body) => 1 + body.size(),
            Expr::App(func, arg) => 1 + func.size() + arg.size(),
        }
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Expr::Var(i) => write!(f, "(Var {})", i),
            Expr::Bool(b) => write!(f, "(Bool {})", if *b { "#t" } else { "#f" }),
            Expr::Abs(typ, body) => write!(f, "(Abs {} {})", typ, body),
            Expr::App(func, arg) => write!(f, "(App {} {})", func, arg),
        }
    }
}

pub type Ctx = Vec<Typ>;

use Expr::*;
use Typ::*;

pub fn get_typ(ctx: &Ctx, expr: &Expr) -> Option<Typ> {
    match expr {
        Var(i) => {
            if *i < 0 {
                None
            } else {
                ctx.get(*i as usize).cloned()
            }
        }
        Bool(_) => Some(TBool),
        Abs(typ, body) => {
            let mut new_ctx = ctx.clone();
            new_ctx.insert(0, typ.clone());
            get_typ(&new_ctx, body).map(|ret| TFun(Box::new(typ.clone()), Box::new(ret)))
        }
        App(func, arg) => {
            let func_type = get_typ(ctx, func)?;
            let arg_type = get_typ(ctx, arg)?;
            match func_type {
                TFun(param_type, ret_type) if *param_type == arg_type => Some(*ret_type),
                _ => None,
            }
        }
    }
}

pub fn type_check(ctx: &Ctx, expr: &Expr, typ: &Typ) -> bool {
    match get_typ(ctx, expr) {
        Some(expr_typ) => expr_typ == *typ,
        None => false,
    }
}

pub fn shift(d: i32, expr: &Expr) -> Expr {
    fn go(c: i32, e: &Expr, d: i32) -> Expr {
        match e {
            Var(i) => {
                /*| */
/*|
                if *i < c { Var(*i) } else { Var(*i + d) }
*/
                /*|| shift_var_none */
                /*|
                Var(*i)
                */
                /*|| shift_var_all */
                /*|
                Var(*i + d)
                */
                /*|| shift_var_leq */
                if *i <= c { Var(*i) }
                else { Var(*i + d) }
                /* |*/
            }
            Bool(b) => Bool(*b),
            Abs(typ, body) => {
                /*| */
                                                Abs(typ.clone(), Box::new(go(c + 1, body, d)))
                /*|| shift_abs_no_incr */
                /*|
                Abs(typ.clone(), Box::new(go(c, body, d)))
                */
                /* |*/
            }
            App(func, arg) => App(Box::new(go(c, func, d)), Box::new(go(c, arg, d))),
        }
    }

    go(0, expr, d)
}

pub fn subst(n: i32, s: &Expr, e: &Expr) -> Expr {
    match e {
        Var(i) => {
            /*| */
            if *i == n { s.clone() } else { Var(*i) }
            /*|| subst_var_all */
            /*|
            s.clone()
            */
            /*|| subst_var_none */
            /*|
            Var(*i)
            */
            /* |*/
        }
        Bool(b) => Bool(*b),
        Abs(typ, body) => {
            /*| */
            Abs(typ.clone(), Box::new(subst(n + 1, &shift(1, s), body)))
            /*|| subst_abs_no_shift */
            /*|
            Abs(typ.clone(), Box::new(subst(n + 1, s, body)))
            */
            /*|| subst_abs_no_incr */
            /*|
            Abs(typ.clone(), Box::new(subst(n, &shift(1, s), body)))
            */
            /* |*/
        }
        App(func, arg) => App(Box::new(subst(n, s, func)), Box::new(subst(n, s, arg))),
    }
}

pub fn subst_top(s: &Expr, e: &Expr) -> Expr {
    /*| */
    shift(-1, &subst(0, &shift(1, s), e))
    /*|| substTop_no_shift */
    /*|
    subst(0, s, e)
    */
    /*|| substTop_no_shift_back */
    /*|
    subst(0, &shift(1, s), e)
    */
    /* |*/
}

pub fn pstep(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Abs(t, e) => {
            let ep = pstep(e)?;
            Some(Expr::Abs(t.clone(), Box::new(ep)))
        }

        Expr::App(box Expr::Abs(_, box e1), box e2) => {
            let e1p = pstep(e1).unwrap_or_else(|| e1.clone());
            let e2p = pstep(e2).unwrap_or_else(|| e2.clone());
            Some(subst_top(&e2p, &e1p))
        }

        Expr::App(box e1, box e2) => match (pstep(e1), pstep(e2)) {
            (None, None) => None,
            (me1, me2) => {
                let new_e1 = me1.unwrap_or_else(|| e1.clone());
                let new_e2 = me2.unwrap_or_else(|| e2.clone());
                Some(Expr::App(Box::new(new_e1), Box::new(new_e2)))
            }
        },

        Expr::Var(_) | Expr::Bool(_) => None,
    }
}

pub fn multistep(f: usize, step: fn(&Expr) -> Option<Expr>, expr: &Expr) -> Option<Expr> {
    let mut current = expr.clone();
    for _ in 0..f {
        if let Some(next) = step(&current) {
            current = next;
        } else {
            return Some(current);
        }
    }
    Some(current)
}

pub fn is_nf(expr: &Expr) -> bool {
    match expr {
        Var(_) | Bool(_) => true,
        Abs(_, body) => is_nf(body),
        App(box Abs(_, _), _) => false, // Application of an abstraction is not in normal form
        App(func, arg) => is_nf(func) && is_nf(arg),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_serialize_deserialize() {
        use super::*;
        use serde_lexpr::{from_str, to_string};

        let expr = Expr::App(
            Box::new(Expr::Abs(Typ::TBool, Box::new(Expr::Var(0)))),
            Box::new(Expr::Bool(true)),
        );

        let serialized = to_string(&expr).unwrap();
        println!("Serialized: {}", serialized);
        let deserialized: Expr = from_str(&serialized).unwrap();

        assert_eq!(expr, deserialized);
    }
}
