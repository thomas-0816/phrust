<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-set-rebind-proc-2d7073fd1a area=builtin_contract kind=function symbol=ldap_set_rebind_proc source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-set-rebind-proc-2d7073fd1a failure_category=builtin_contract requires_ref_extension=ldap
try {
    $result = \ldap_set_rebind_proc();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
