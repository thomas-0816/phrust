<?php
// oracle-probe: id=oracle-callable-dispatch-function-variable-function-a0bcd90cba area=callable_dispatch kind=function symbol=variable-function source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-callable-dispatch-function-variable-function-a0bcd90cba failure_category=callable_dispatch
$fn = "strlen"; echo $fn("abcd"), "\n";
