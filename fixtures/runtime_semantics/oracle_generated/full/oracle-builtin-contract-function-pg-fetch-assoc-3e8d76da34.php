<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-fetch-assoc-3e8d76da34 area=builtin_contract kind=function symbol=pg_fetch_assoc source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-fetch-assoc-3e8d76da34 failure_category=builtin_contract requires_ref_extension=pgsql
$name = "pg_fetch_assoc";
echo function_exists($name) ? "available\n" : "missing\n";
