#lang racket

(provide (all-defined-out))

(require "Impl.rkt")

;; tree A -> listof real
(define (keys t)
  (match t
    [(E) '()]
    [(T l k v r) (append (keys l) (list k) (keys r))]))

;; tree A -> bool
(define (BST? t)
  (match t
    [(E) #t]
    [(T l k v r)
     (if (and (BST? l) (BST? r))
         (and (andmap (lambda (k1) (< k1 k)) (keys l))
              (andmap (lambda (k1) (> k1 k)) (keys r)))
         #f)]))

;; tree A -> listof A
(define (tree->list t)
  (match t
    [(E) null]
    [(T l k v r) (append (tree->list l) (list (list k v)) (tree->list r))]))

;; (or bool 'nothing) -> (or bool 'nothing)
(define (assumes p1 p2)
  (match p1
    [(nothing) nothing]
    [#f nothing]
    [#t p2]))

#| ----------- |#

#| -- Model-based Properties |#

(define (remove-key k l)
  (filter (lambda (kv) (not (= (car kv) k))) l))

(define (sorted? l)
  (match l
    ['() #t]
    [(cons (cons k v) l2)
     (match l2
       ['() #t]
       [(cons (cons k2 v2) l3)
        (and (< k k2) (sorted? l2))])]))

(define (insert-sorted k v l)
  (match l
    ['() (list (cons k v))]
    [(cons (cons k1 v1) rst)
     (cond
       [(= k k1) (cons (cons k v) rst)]
       [(< k k1) (cons (cons k v) l)]
       [else (cons (cons k1 v1) (insert-sorted k v rst))])]))

(define (union-sorted l1 l2)
  (match l1
    ['() l2]
    [(cons (cons k v) rst1)
     (union-sorted rst1 (insert-sorted k v l2))]))

#| ----------- |#

(define (tree-equiv? t1 t2)
  (just (equal? (tree->list t1) (tree->list t2))))

#| ----------- |#

#| -- Metamorphic Properties |#

#| ----------- |#

(define (size-BST t)
  (length (tree->list t)))

#| ----------- |#

#| -- Size Properties |#


#| ----------- |#

#| -- Validity Properties. |#

(define (prop_InsertValid t k v)
  (assumes (BST? t) (just (BST? ((insert k v t)))))
  )

(define (prop_DeleteValid t k)
  (assumes (BST? t) (just (BST? (delete k t))))
  )

(define (prop_UnionValid t1 t2)
  (assumes
   (match (cons (BST? t1) (BST? t2))
     [(cons #t #t) #t]
     [* (nothing)]
     )
   (just (BST? (union t1 t2))))
  )

#| ----------- |#

#| -- Post-condition Properties |#

(define (prop_InsertPost t k1 k2 v)
  (assumes (BST? t)
           (just (equal? (find k2 (insert k1 v t)) (if (equal? k1 k2) (just v) (find k2 t))))
           )
  )

(define (prop_DeletePost t k1 k2)
  (assumes (BST? t)
           (just (equal? (find k2 (delete k1 t)) (if (= k1 k2) (nothing) (find k2 t))))
           )
  )

(define (prop_UnionPost t1 t2 k)
  (assumes
   (match (cons (BST? t1) (BST? t2))
     [(cons #t #t) #t]
     [* (nothing)]
     )
   (let ([search-union (find k (union t1 t2))]
         [search-t1 (find k t1)]
         [search-t2 (find k t2)])
     (if (just? search-t1)
         (just (equal? search-union search-t1))
         (just (equal? search-union search-t2))
         )
     ))
  )

#| ----------- |#

#| -- Model-based Properties |#

(define (deleteKey k l)
  (filter (lambda (kv) (not (= (first kv) k))) l)
  )

(define (sorted l)
  (match l
    ['() #t]
    [(cons (cons k v) l2) (match l2
                            [null #t]
                            [(cons (cons k2 v2) l3) (and (< k k2) (sorted l2))]
                            )
                          ]
    )
  )

(define (l_insert k v l)
  (match l
    ['() (list (list k v))]
    [(cons (list k1 v1) xs) (cond
                              [(= k k1) (cons (list k v) xs)]
                              [(< k k1) (cons (list k v) l)]
                              [else (append (list (list k1 v1)) (l_insert k v xs))]
                              )
                            ]
    )
  )

(define (l_sort l)
  (match l
    ['() null]
    [(cons (list k v) l2) (l_insert k v (l_sort l2))]
    )
  )

(define  (l_find k l)
  (match (filter (lambda (kv) (= (first kv) k)) l)
    ['() (nothing)]
    [(cons kv *) (just (second kv))]
    )
  )

(define (l_unionBy f l1 l2)
  (match l1
    ['() l2]
    [(cons (list k v) l1_2)
     (let ([l2_2 (deleteKey k l2)])
       (let ([v1 (match (l_find k l2) [(nothing) v] [(just v1) (f v v1)])])
         (l_insert k v1 (l_unionBy f l1_2 l2_2))))
     ]
    )
  )

(define (prop_InsertModel t k v)
  (assumes (BST? t) (just (equal? (tree->list (insert k v t)) (l_insert k v (deleteKey k (tree->list t))))))
  )

(define (prop_DeleteModel t k)
  (assumes (BST? t) (just (equal? (tree->list (delete k t)) (deleteKey k (tree->list t)))))
  )

(define (prop_UnionModel t1 t2)
  (assumes (match (cons (BST? t1) (BST? t2))
             [(cons #t #t) #t]
             [* (nothing)]
             )
           (just (equal? (tree->list (union t1 t2))
                         (l_sort (l_unionBy (lambda (x *) x) (tree->list t1) (tree->list t2)))
                         )
                 )
           )
  )

#| ----------- |#

#| -- Metamorphic Properties |#

(define (prop_InsertInsert t k1 k2 v1 v2)
  (assumes (BST? t) (tree-equiv? (insert k1 v1 (insert k2 v2 t))
                              (if (= k1 k2) (insert k1 v1 t) (insert k2 v2 (insert k1 v1 t)))
                              )

           )
  )

(define (prop_InsertDelete t k1 k2 v)
  (assumes (BST? t) (tree-equiv? (insert k1 v (delete k2 t))
                              (if (= k1 k2) (insert k1 v t) (delete k2 (insert k1 v t)))
                              )

           )
  )

(define (prop_InsertUnion t1 t2 k v)
  (assumes (match (cons (BST? t1) (BST? t2))
             [(cons #t #t) #t]
             [* (nothing)]
             )
           (tree-equiv? (insert k v (union t1 t2)) (union (insert k v t1) t2))
           )
  )

(define (prop_DeleteInsert t k1 k2 v)
  (assumes (BST? t) (tree-equiv? (delete k1 (insert k2 v t))
                              (if (= k1 k2) (delete k1 t) (insert k2 v (delete k1 t)))
                              )
           )
  )

(define (prop_DeleteDelete t k1 k2)
  (assumes (BST? t) (tree-equiv? (delete k1 (delete k2 t))
                              (delete k2 (delete k1 t))
                              )
           )
  )

(define (prop_DeleteUnion t1 t2 k)
  (assumes (and (BST? t1) (BST? t2))
           (tree-equiv? (delete k (union t1 t2)) (union (delete k t1) (delete k t2)))
           )
  )

(define (prop_UnionDeleteInsert t1 t2 k v)
  (assumes (and (BST? t1) (BST? t2))
           (tree-equiv? (union (delete k t1) (insert k v t2)) (insert k v (union t1 t2)))
           )
  )

(define (prop_UnionUnionIdem t)
  (assumes (BST? t) (tree-equiv? (union t t) t))
  )

(define (prop_UnionUnionAssoc t1 t2 t3)
  (assumes (and (BST? t1) (BST? t2) (BST? t3))
           (just (equal? (union (union t1 t2) t3) (union t1 (union t2 t3))))
           )
  )

