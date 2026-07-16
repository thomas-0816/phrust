<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-count-references-7d3b16cbb9 area=builtin_contract kind=function symbol=ldap_count_references source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-count-references-7d3b16cbb9 failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_count_references";
echo function_exists($name) ? "available\n" : "missing\n";
