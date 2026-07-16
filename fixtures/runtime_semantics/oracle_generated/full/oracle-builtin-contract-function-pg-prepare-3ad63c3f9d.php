<?php
// oracle-probe: id=oracle-builtin-contract-function-pg-prepare-3ad63c3f9d area=builtin_contract kind=function symbol=pg_prepare source=ext/pgsql/pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pg-prepare-3ad63c3f9d failure_category=builtin_contract requires_ref_extension=pgsql
$name = "pg_prepare";
echo function_exists($name) ? "available\n" : "missing\n";
