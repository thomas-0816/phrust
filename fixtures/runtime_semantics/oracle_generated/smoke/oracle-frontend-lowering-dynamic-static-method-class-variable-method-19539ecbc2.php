<?php
// oracle-probe: id=oracle-frontend-lowering-dynamic-static-method-class-variable-method-19539ecbc2 area=frontend_lowering kind=dynamic_static_method symbol=class-variable-method source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-frontend-lowering-dynamic-static-method-class-variable-method-19539ecbc2 failure_category=frontend_lowering
class OracleStaticCallBox { public static function label(): string { return "ok"; } }
$class = OracleStaticCallBox::class;
$method = "label";
echo $class::$method(), "\n";
