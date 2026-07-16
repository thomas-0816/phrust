<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-read-67a784d9df area=builtin_contract kind=function symbol=ldap_read source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-read-67a784d9df failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_read";
echo function_exists($name) ? "available\n" : "missing\n";
