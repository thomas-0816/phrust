<?php
// oracle-probe: id=oracle-builtin-contract-function-msg-send-8348f1a945 area=builtin_contract kind=function symbol=msg_send source=ext/sysvmsg/sysvmsg.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msg-send-8348f1a945 failure_category=builtin_contract requires_ref_extension=sysvmsg
try {
    $result = \msg_send();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
