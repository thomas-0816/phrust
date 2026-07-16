<?php
// oracle-probe: id=oracle-callable-dispatch-method-dynamic-class-variable-static-call-aa9882bc47 area=callable_dispatch kind=method symbol=dynamic-class-variable-static-call source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-callable-dispatch-method-dynamic-class-variable-static-call-aa9882bc47 failure_category=callable_dispatch
class OracleCallableBox { public static function wrap($value) { return "[" . $value . "]"; } }
$class = OracleCallableBox::class;
$callable = [$class, "wrap"];
echo $callable("x"), "\n";
