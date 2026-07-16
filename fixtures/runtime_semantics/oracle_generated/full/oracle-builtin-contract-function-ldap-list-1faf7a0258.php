<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-list-1faf7a0258 area=builtin_contract kind=function symbol=ldap_list source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-list-1faf7a0258 failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_list";
echo function_exists($name) ? "available\n" : "missing\n";
