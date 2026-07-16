<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-escape-bytea-f60a5830e6 area=builtin_contract kind=function symbol=pg_escape_bytea source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-escape-bytea-f60a5830e6 failure_category=builtin_contract requires_ref_extension=pgsql
$name = "pg_escape_bytea";
echo function_exists($name) ? "available\n" : "missing\n";
