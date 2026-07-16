<?php
// oracle-probe: id=oracle-builtin-contract-function-msg-stat-queue-7d4b3d6452 area=builtin_contract kind=function symbol=msg_stat_queue source=ext/sysvmsg/sysvmsg.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msg-stat-queue-7d4b3d6452 failure_category=builtin_contract requires_ref_extension=sysvmsg
try {
    $result = \msg_stat_queue();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
