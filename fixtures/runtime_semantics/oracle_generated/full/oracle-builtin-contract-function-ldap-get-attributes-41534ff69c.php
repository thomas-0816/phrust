<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-get-attributes-41534ff69c area=builtin_contract kind=function symbol=ldap_get_attributes source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-get-attributes-41534ff69c failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_get_attributes";
echo function_exists($name) ? "available\n" : "missing\n";
