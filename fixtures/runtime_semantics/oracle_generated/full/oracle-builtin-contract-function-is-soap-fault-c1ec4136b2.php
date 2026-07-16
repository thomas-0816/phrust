<?php
// oracle-probe: id=oracle-builtin-contract-function-is-soap-fault-c1ec4136b2 area=builtin_contract kind=function symbol=is_soap_fault source=ext/soap/soap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-is-soap-fault-c1ec4136b2 failure_category=builtin_contract requires_ref_extension=soap
try {
    $result = \is_soap_fault();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
