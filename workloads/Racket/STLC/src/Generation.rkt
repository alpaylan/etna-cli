#lang racket

(require "Impl.rkt")
(require "Spec.rkt")
(require rackcheck)
(require data/maybe)

(define nat-lst? (list*of positive?))

(define (gen:bind-opt g f)
  (gen:bind g
            (lambda (maybe-x)
              (match maybe-x
                [(nothing) (gen:const nothing)]
                [(just x) (f x)]))))

(define/contract (list-pop ls index)
  (-> (listof any/c) exact-integer? (values any/c (listof any/c)))
    (if (> (+ index 1) (length ls))
        (values (raise-argument-error) ls)
        (match-let ([(cons weight gen) (list-ref ls index)])
          (if (= weight 1)
            (values gen (drop ls index))
            (values gen (list-set ls index (cons (- weight 1) gen)))))))

(define (backtrack gs)
  (define (backtrack-iter gs)
    (if (null? gs)
        (gen:const nothing)
        ; Pull a random generator from the list
        (let [(index (random 0 (length gs)))]
          (let-values ([(g gs2) (list-pop gs index)])
            (gen:bind g
                      (lambda (x)
                        (match x
                          [(nothing) (backtrack-iter gs2)]
                          [(just x) (gen:const (just x))])))))))
  (backtrack-iter gs))

(define (gen:var ctx t p r)
  (match ctx
    ['() r]
    [(cons t2 ctx2) (if (equal? t t2)
                        (gen:var ctx2 t (+ p 1) (cons p r))
                        (gen:var ctx2 t (+ p 1) r))]))

(define (gen:vars ctx t)
  (let [(var-nats (gen:var ctx t 0 '()))]
    (map (lambda (p) (gen:const (just (Var p)))) var-nats)))

(define (gen:one env tau)
  (match tau
    [(TBool) (gen:bind gen:boolean (lambda (b) (gen:const (just (Bool b)))))]
    [(TFun T1 T2) (gen:bind-opt (gen:one (cons T1 env) T2)
                                (lambda (e) (gen:const (just (Abs T1 e)))))]))

(define (gen:typ size)
  (match size
    [0 (gen:const (TBool))]
    [n (gen:one-of-total nothing 
                        (list 
                          (gen:typ (quotient n 2)) 
                          (gen:bind 
                            (gen:delay (gen:typ (quotient n 2)))
                              (lambda (T1) (gen:bind (gen:delay (gen:typ (quotient n 2)))
                                               (lambda (T2) (gen:const (TFun T1 T2))))))))]))

(define/contract (gen:one-of-total fallback gs)
  (-> any/c (listof gen?) gen?)
  (if (null? gs)
      (gen:const fallback)
      (apply gen:choice gs)))

(define/contract (gen:expr env tau sz)
  (-> (listof typ?) typ? number? gen?)
  (match sz
    [0 (gen:one-of-total nothing (list (gen:one-of-total nothing (gen:vars env tau))
                                       (gen:one env tau)))]
    [n (backtrack  (list  (cons 1 (gen:one env tau))
                          (cons 1 (gen:one-of-total nothing (gen:vars env tau)))
                          (cons 2 (gen:bind (gen:typ (random 1 (+ 1 (min n 10))))
                                    (lambda (T1) (gen:bind-opt (gen:expr env (TFun T1 tau) (quotient n 2))
                                                                (lambda (e1) (gen:bind-opt (gen:expr env T1 (quotient n 2))
                                                                                          (lambda (e2) (gen:const (just (App e1 e2))))))))))                                            
                          (cons 2 (match tau
                                  [(TBool) (gen:bind gen:boolean (lambda (b) (gen:const (just (Bool b)))))]
                                  [(TFun T1 T2) (gen:bind-opt (gen:expr (cons T1 env) T2 (quotient n 2))
                                                              (lambda (e) (gen:const (just (Abs T1 e)))))]))))]))

(define gSized
  (gen:bind (gen:typ 150)
            (lambda (tau)
              (gen:bind-opt (gen:expr '() tau 100) (lambda (x) (gen:const x))))))

(provide gSized)