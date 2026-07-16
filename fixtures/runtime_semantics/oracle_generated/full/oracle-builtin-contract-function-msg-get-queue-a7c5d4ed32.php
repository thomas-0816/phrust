<?php
// oracle-probe: id=oracle-builtin-contract-function-msg-get-queue-a7c5d4ed32 area=builtin_contract kind=function symbol=msg_get_queue source=ext/sysvmsg/sysvmsg.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msg-get-queue-a7c5d4ed32 failure_category=builtin_contract requires_ref_extension=sysvmsg
try {
    $result = \msg_get_queue();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
