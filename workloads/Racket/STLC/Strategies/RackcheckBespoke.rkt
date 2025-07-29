#lang racket

(require "../src/Spec.rkt")
(require "../src/Generation.rkt")
(require rackcheck)
(require data/maybe)

(define (truthy? x)
  (match x
    [(nothing) #t]
    [#t #t]
    [(just #t) #t]
    [(just #f) #f]
    [#f #f]))

(define test_prop_SinglePreserve
  (property prop_SinglePreserve ([e gSized])
            (truthy? (prop_SinglePreserve e)))
  )


(define test_prop_MultiPreserve
  (property prop_MultiPreserve ([e gSized])
            (truthy? (prop_MultiPreserve e)))
  )

(provide test_prop_SinglePreserve
         test_prop_MultiPreserve)

