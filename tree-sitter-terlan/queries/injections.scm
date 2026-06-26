; Tree-sitter injections for Terlan template expression islands.
;
; Inputs:
; - Interpolation nodes parsed from `.terl.*` template regions.
;
; Outputs:
; - Terlan language injection for the expression inside `${...}`.
;
; Transformation:
; - Marks the interpolation expression as Terlan source so editor hosts can
;   reuse Terlan highlighting inside mixed template files.
(interpolation
  (expression) @injection.content
  (#set! injection.language "terlan"))
