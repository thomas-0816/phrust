<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-escape-16f3f93702 area=builtin_contract kind=function symbol=ldap_escape source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-escape-16f3f93702 failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_escape";
echo function_exists($name) ? "available\n" : "missing\n";
