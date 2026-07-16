<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-free-result-d404774d17 area=builtin_contract kind=function symbol=pg_free_result source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-free-result-d404774d17 failure_category=builtin_contract requires_ref_extension=pgsql
$name = "pg_free_result";
echo function_exists($name) ? "available\n" : "missing\n";
