#lang racket

(provide (all-defined-out))
(require data/maybe)
(require data/monad)
(require racket/struct)

(struct TBool () 
    #:property prop:custom-write
  (lambda (p port mode)
      (fprintf port "(TBool)"))
  #:transparent)
      
(struct TFun (t1 t2) 
    #:property prop:custom-write
  (lambda (p port mode)
      (fprintf port "(TFun ~a ~a)" (TFun-t1 p) (TFun-t2 p)))
  #:transparent)

(struct Var (nat) 
#:property prop:custom-write
  (lambda (p port mode)
      (fprintf port "(Var ~a)" (Var-nat p)))
  #:transparent)
        
(struct Bool (bool) 
    #:property prop:custom-write
    (lambda (p port mode)
        (fprintf port "(Bool ~a)" (Bool-bool p)))
    #:transparent)
        
(struct Abs (t1 e2)
    #:property prop:custom-write
    (lambda (p port mode)
        (fprintf port "(Abs ~a ~a)" (Abs-t1 p) (Abs-e2 p)))
    #:transparent)

(struct App (e1 e2) 
    #:property prop:custom-write
    (lambda (p port mode)
        (fprintf port "(App ~a ~a)" (App-e1 p) (App-e2 p)))
    #:transparent)

(define expr? (lambda (x) (or (Var? x) (Bool? x) (Abs? x) (App? x))))
(define typ? (lambda (x) (or (TBool? x) (TFun? x))))
(define ctx? (lambda (x) (or (empty? x) ((list*of typ?) x))))


(define/contract (get-typ ctx e)
    (ctx? expr? . -> . (maybe/c typ?))
    (match e 
        [(Var n) (if (and (>= n 0) (< n (length ctx))) (just (list-ref ctx n)) nothing)]
        [(Bool _) (just (TBool))]
        [(Abs t e) (do [t2 <- (get-typ (cons t ctx) e)] (just (TFun t t2)))]
        [(App e1 e2) (do [t1 <- (get-typ ctx e1)] (match t1 
                                                    [(TFun t11 t12) (do [t2 <- (get-typ ctx e2)] (if (equal? t11 t2) (just t12) nothing))]
                                                    [_ nothing]
                                                  ))]
    )
)

(define/contract (type-check ctx e t)
(ctx? expr? typ? . -> . boolean?) 
    (match (get-typ ctx e)
        [(just t2) (equal? t t2)]
        [(nothing) #f]
    )
)

(define/contract (shift_ c e d)
(number? expr? number? . -> . expr?)
    (match e 
        [(Var n) (
            #|! |#
#|!
            if (< n c) (Var n) (Var (+ n d))
|#
            #|!! shift_var_none |#
            #|!
Var n
            |#
            #|!! shift_var_all |#
            #|!
Var (+ n d)
            |#
            #|!! shift_var_leq |#
if (<= n c) (Var n) (Var (+ n d))
            #| !|#
            )]
        [(Bool b) (Bool b)]
        [(Abs t e) (
            #|! |#
            Abs t (shift_ (+ c 1) e d)
            #|!! shift_abs_no_incr |#
            #|!
            Abs t (shift_ c e d)
            |#
            #| !|#
        )]
        [(App e1 e2) (App (shift_ c e1 d) (shift_ c e2 d))]
    )
) 
(define/contract (shift d ex)
(number? expr? . -> . expr?)
    (shift_ 0 ex d)
)

(define/contract (subst n s e)
(number? expr? expr? . -> . expr?)
    (match (list n s e)
        [(list n s (Var m)) 
        #|! |#
        (if (= m n) s (Var m))
        #|!! subst_var_all |#
        #|!
s
        |#
        #|!! subst_var_none |#
        #|!
(Var m)
        |#
        #| !|#
        ]
        [(list _ _ (Bool b)) (Bool b)]
        [(list n s (Abs t e )) (
        #|! |#
        Abs t (subst (+ n 1) (shift 1 s) e)
        #|!! subst_abs_no_shift |#
        #|!
Abs t (subst (+ n 1) s e)
        |#
        #|!! subst_abs_no_incr |#
        #|!
Abs t (subst n (shift 1 s) e)
        |#
        #| !|#
        )]
        [(list n s (App e1 e2)) (App (subst n s e1) (subst n s e2))]
    )
)

(define/contract (subst-top s e)
(expr? expr? . -> . expr?)
    #|! |#
    (shift -1 (subst 0 (shift 1 s) e))
    #|!! substTop_no_shift |#
    #|!
(subst 0 s e)
    |#
    #|!! substTop_no_shift_back |#
    #|!
(subst 0 (shift 1 s) e)
    |#
    #| !|#
)

(define (from-maybe a1 a2)
    (match a2
        [(just v) v]
        [(nothing) a1]
    )
)

(define/contract (pstep e)
(expr? . -> . (maybe/c expr?))
    (match e 
        [(Abs t e) (do [e2 <- (pstep e)] (just (Abs t e2)))]
        [(App (Abs _ e1) e2) (let* ([e11 (from-maybe e1 (pstep e1))]
                                    [e21 (from-maybe e2 (pstep e2))])
                                (just (subst-top e21 e11)))]
        [(App e1 e2) (let* ([e1-res (pstep e1)]
                            [e2-res (pstep e2)])
                        (match (list e1-res e2-res)
                            [(list nothing nothing) nothing]
                            [(list me1 me2) (let* ([e11 (from-maybe e1 me1)]
                                                    [e21 (from-maybe e2 me2)])
                                                (just (App e11 e21)))]))]
        [_ nothing])
    )


(define/contract (multistep step e)
(-> (-> expr? (maybe/c expr?)) expr? (maybe/c expr?)) ; not sure abt the contract
    (match (step e)
        [(nothing) (just e)]
        [(just e2) (multistep step e2)]
    )
)

(define/contract (is-NF e)
(expr? . -> . boolean?)
    (match e 
        [(Var _) #t]
        [(Bool _) #t]
        [(Abs _ e) (is-NF e)]
        [(App (Abs _ _) _) #f]
        [(App e1 e2) (and (is-NF e1) (is-NF e2))]
    )
)