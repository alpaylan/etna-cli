#lang racket

(require "../src/Impl.rkt")
(require "../src/Spec.rkt")
(require "../src/Generation.rkt")
(require rackcheck)
(require rackunit)
(provide (all-defined-out))

#| Validity Properties |#

(define truthy?
  (λ (x) (or (equal? x #t) (equal? x (just #t)))))

(define test_prop_InsertValid
    (property insertValid ([t bespoke] [k gen:natural] [v gen:natural])
        (truthy? (prop_InsertValid t k v)))
)

(define test_prop_DeleteValid
    (property deleteValid ([t bespoke] [k gen:natural])
        (truthy? (prop_DeleteValid t k)))
)

(define test_prop_UnionValid
    (property unionValid ([t1 bespoke] [t2 bespoke])
        (truthy? (prop_UnionValid t1 t2)))
)

#| Post-condition Properties |#

(define test_prop_InsertPost
    (property insertPost ([t bespoke] [k1 gen:natural] [k2 gen:natural] [v gen:natural])
        (truthy? (prop_InsertPost t k1 k2 v)))
)

(define test_prop_DeletePost
    (property deletePost ([t bespoke] [k1 gen:natural] [k2 gen:natural])
        (truthy? (prop_DeletePost t k1 k2)))
)

(define test_prop_UnionPost
    (property unionPost ([t1 bespoke] [t2 bespoke] [k gen:natural])
        (truthy? (prop_UnionPost t1 t2 k)))
)

#| Model-based Properties |#

(define test_prop_InsertModel
    (property insertModel ([t bespoke] [k gen:natural] [v gen:natural])
        (truthy? (prop_InsertModel t k v)))
)

(define test_prop_DeleteModel
    (property deleteModel ([t bespoke] [k gen:natural])
        (truthy? (prop_DeleteModel t k)))
)

(define test_prop_UnionModel
    (property unionModel ([t1 bespoke] [t2 bespoke])
        (truthy? (prop_UnionModel t1 t2)))
)

#| Metamorphic Properties |#

(define test_prop_InsertInsert
    (property insertInsert ([t bespoke] [k1 gen:natural] [k2 gen:natural] [v1 gen:natural] [v2 gen:natural])
        (truthy? (prop_InsertInsert t k1 k2 v1 v2)))
)

(define test_prop_InsertDelete
    (property insertDelete ([t bespoke] [k1 gen:natural] [k2 gen:natural] [v gen:natural])
        (truthy? (prop_InsertDelete t k1 k2 v)))
)

(define test_prop_InsertUnion
    (property insertUnion ([t1 bespoke] [t2 bespoke] [k gen:natural] [v gen:natural])
        (truthy? (prop_InsertUnion t1 t2 k v)))
)

(define test_prop_DeleteInsert
    (property deleteInsert ([t bespoke] [k1 gen:natural] [k2 gen:natural] [v gen:natural])
        (truthy? (prop_DeleteInsert t k1 k2 v)))
)

(define test_prop_DeleteDelete
    (property deleteDelete ([t bespoke] [k1 gen:natural] [k2 gen:natural])
        (truthy? (prop_DeleteDelete t k1 k2)))
)

(define test_prop_DeleteUnion
    (property deleteUnion ([t1 bespoke] [t2 bespoke] [k gen:natural])
        (truthy? (prop_DeleteUnion t1 t2 k)))
)

(define test_prop_UnionDeleteInsert
    (property unionDeleteInsert ([t1 bespoke] [t2 bespoke] [k gen:natural] [v gen:natural])
        (truthy? (prop_UnionDeleteInsert t1 t2 k v)))
)

(define test_prop_UnionUnionIdem
    (property unionUnionIdem ([t bespoke])
        (truthy? (prop_UnionUnionIdem t)))
)

(define test_prop_UnionUnionAssoc
    (property unionUnionAssoc ([t1 bespoke] [t2 bespoke] [t3 bespoke])
        (truthy? (prop_UnionUnionAssoc t1 t2 t3)))
)