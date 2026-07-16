<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-affected-rows-3f4755aba1 area=builtin_contract kind=function symbol=pg_affected_rows source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-affected-rows-3f4755aba1 failure_category=builtin_contract requires_ref_extension=pgsql
$name = "pg_affected_rows";
echo function_exists($name) ? "available\n" : "missing\n";
