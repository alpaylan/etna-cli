#lang racket

(require "Impl.rkt")
(require data/maybe)

(define (assumes p1 p2)
  (match p1
    [(nothing) nothing]
    [#f nothing]
    [(just #f) nothing]
    [(just #t) p2]
    [#t p2]
  )
)

(define/contract (mt e)
(expr? . -> . (maybe/c typ?))
    (get-typ null e) ; is it null??
)

(define (is-just a)
    (match a 
        [(just _) #t]
        [(nothing) #f]
    )
)

(define/contract (mtypecheck e t)
((maybe/c expr?) typ? . -> . boolean?)
    (match e 
        [(just e2) (type-check null e2 t)]
        [(nothing) #t]
    )
)


(define (apply_maybe f x)
  (match x
    [(just x) (f x)]
    [(nothing) (nothing)]
  )
)

(define (prop_SinglePreserve e)
    (assumes (is-just (mt e)) (apply_maybe (lambda (t2) (just (mtypecheck (pstep e) t2))) (mt e)))
)

(define (prop_MultiPreserve e)
    (assumes (is-just (mt e)) (apply_maybe (lambda (t2) (mtypecheck (multistep pstep e) t2)) (mt e)))
)

(define/contract (size-STLC e)
(expr? . -> . number?)
    (match e 
        [(Abs _ e) (+ 1 (size-STLC e))]
        [(App e1 e2) (+ 1 (size-STLC e1) (size-STLC e2))]
        [_ 1]
    )
)

(provide prop_SinglePreserve prop_MultiPreserve size-STLC)