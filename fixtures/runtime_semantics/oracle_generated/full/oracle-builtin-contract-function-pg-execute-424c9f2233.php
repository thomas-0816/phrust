<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-execute-424c9f2233 area=builtin_contract kind=function symbol=pg_execute source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-execute-424c9f2233 failure_category=builtin_contract requires_ref_extension=pgsql
$name = "pg_execute";
echo function_exists($name) ? "available\n" : "missing\n";
