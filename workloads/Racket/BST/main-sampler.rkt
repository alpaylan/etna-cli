
#lang racket

(require racket/cmdline)
(require racket/format)
(require racket/match)
(require "./src/Impl.rkt")
(require "./src/Spec.rkt")
(require "./Strategies/RackcheckBespoke.rkt")
(require "src/Generation.rkt")
(require rackcheck)
(require pretty-format)
(require (only-in racket/base (apply racket-apply)))

(define (usage)
  (displayln (format "Usage: ~s <tool> <property> <tests>" (path->string (find-system-path 'run-file))))
  (displayln "Available tools: rackcheck")
  (displayln "For available properties, check https://github.com/alpaylan/etna-cli/blob/main/docs/workloads/bst.md"))

(define (parse-args)
  (define args (current-command-line-arguments))
  (if (< (vector-length args) 3)
      (begin (usage) (exit 1))
      (list (vector-ref args 0)
            (vector-ref args 1)
            (string->number (vector-ref args 2)))))


(define prop-generators
  `(
    ("insertValid"        . ,       (gen:let ([t bespoke] [k gen:natural] [v gen:natural]) 
                                            (insert k v t)))
    ("deleteValid"        . ,       (gen:let ([t bespoke] [k gen:natural])
                                            (delete k t))) 
    ("unionValid"         . ,       (gen:let ([t1 bespoke] [t2 bespoke])
                                            (union t1 t2)))                                       
    ("insertPost"         . ,       (gen:let ([t bespoke] [k1 gen:natural] [k2 gen:natural] [v gen:natural])
                                            (insert k1 v t)))  
    ("deletePost"         . ,       (gen:let ([t bespoke] [k1 gen:natural] [k2 gen:natural])
                                            (delete k1 t)))           
    ("unionPost"          . ,       (gen:let ([t1 bespoke] [t2 bespoke] [k gen:natural])
                                            (union t1 t2)))
    ("insertModel"        . ,       (gen:let ([t bespoke] [k gen:natural] [v gen:natural])
                                            (insert k v t)))
    ("deleteModel"        . ,       (gen:let ([t bespoke] [k gen:natural])
                                            (delete k t)))
    ("unionModel"         . ,       (gen:let ([t1 bespoke] [t2 bespoke])
                                            (union t1 t2)))
    ("insertInsert"       . ,       (gen:let ([t bespoke] [k1 gen:natural] [k2 gen:natural] [v1 gen:natural] [v2 gen:natural])
                                            (insert k1 v1 (insert k2 v2 t))))
    ("insertDelete"       .,        (gen:let ([t bespoke] [k1 gen:natural] [k2 gen:natural] [v gen:natural])
                                            (insert k1 v (delete k2 t))))
    ("insertUnion"        .,        (gen:let ([t1 bespoke] [t2 bespoke] [k gen:natural] [v gen:natural]) 
                                            (insert k v (union t1 t2))))  
    ("deleteInsert"       .,        (gen:let ([t bespoke] [k1 gen:natural] [k2 gen:natural] [v gen:natural])
                                            (delete k1 (insert k2 v t))))
    ("deleteDelete"       .,        (gen:let ([t bespoke] [k1 gen:natural] [k2 gen:natural])
                                            (delete k1 (delete k2 t))))
    ("deleteUnion"        .,        (gen:let ([t1 bespoke] [t2 bespoke] [k gen:natural])
                                            (delete k (union t1 t2))))                                          
    ("unionDeleteInsert"  .,        (gen:let ([t1 bespoke] [t2 bespoke] [k gen:natural] [v gen:natural])
                                            (union (delete k t1) (insert k v t2))))
    ("unionUnionIdem"     .,        (gen:let ([t bespoke])
                                            (union (union t t))))
    ("unionUnionAssoc"    .,        (gen:let ([t1 bespoke] [t2 bespoke] [t3 bespoke])
                                            (union (union t1 t2) t3)))
  )
)            

(define (rc-sample property num-tests)
    (print-sample (sample-with-time (dict-ref prop-generators property) num-tests)))

(define (print-sample sample-time-list)
    (pretty-printf "\"time\": ~a\n" (cdr sample-time-list))
    (display "\"values\": ")
    (for-each (lambda (sample) (pretty-printf "~v" sample))
              (car sample-time-list))
    (newline))              

(define (main)
  (define-values (tool property tests) (racket-apply values (parse-args)))
  (define result
    (match (list tool property)
      [(list "rackcheck" "InsertValid")
       (rc-sample "insertValid" tests)]
      [(list "rackcheck" "DeleteValid")
       (rc-sample "deleteValid" tests)]
      [(list "rackcheck" "UnionValid")
       (rc-sample "unionValid" tests)]
      [(list "rackcheck" "InsertPost")
       (rc-sample "insertPost" tests)]
      [(list "rackcheck" "DeletePost")
       (rc-sample "deletePost" tests)]
      [(list "rackcheck" "UnionPost")
       (rc-sample "unionPost" tests)]
      [(list "rackcheck" "InsertModel")
       (rc-sample "insertModel" tests)]
      [(list "rackcheck" "DeleteModel")
       (rc-sample "deleteModel" tests)]
      [(list "rackcheck" "UnionModel")
       (rc-sample "unionModel" tests)]
      [(list "rackcheck" "InsertInsert")
       (rc-sample "insertInsert" tests)]
      [(list "rackcheck" "InsertDelete")
       (rc-sample "insertDelete" tests)]
      [(list "rackcheck" "InsertUnion")
       (rc-sample "insertUnion" tests)]
      [(list "rackcheck" "DeleteInsert")
       (rc-sample "deleteInsert" tests)]
      [(list "rackcheck" "DeleteDelete")
       (rc-sample "deleteDelete" tests)]
      [(list "rackcheck" "DeleteUnion")
       (rc-sample "deleteUnion" tests)]
      [(list "rackcheck" "UnionDeleteInsert")
       (rc-sample "unionDeleteInsert" tests)]
      [(list "rackcheck" "UnionUnionIdem")
       (rc-sample "unionUnionIdem" tests)]
      [(list "rackcheck" "UnionUnionAssoc")
       (rc-sample "unionUnionAssoc" tests)]
      [else (error 'main (format "Unknown tool or property: ~a ~a" tool property))]))
  result
)

(main)

