<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-fetch-result-d5e266ccef area=builtin_contract kind=function symbol=pg_fetch_result source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-fetch-result-d5e266ccef failure_category=builtin_contract requires_ref_extension=pgsql
$name = "pg_fetch_result";
echo function_exists($name) ? "available\n" : "missing\n";
