<?php
// oracle-probe: id=oracle-builtin-contract-function-msg-queue-exists-60c4edad66 area=builtin_contract kind=function symbol=msg_queue_exists source=ext/sysvmsg/sysvmsg.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msg-queue-exists-60c4edad66 failure_category=builtin_contract requires_ref_extension=sysvmsg
try {
    $result = \msg_queue_exists();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
