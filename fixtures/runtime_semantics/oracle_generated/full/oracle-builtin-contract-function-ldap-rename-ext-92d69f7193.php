<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-rename-ext-92d69f7193 area=builtin_contract kind=function symbol=ldap_rename_ext source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-rename-ext-92d69f7193 failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_rename_ext";
echo function_exists($name) ? "available\n" : "missing\n";
