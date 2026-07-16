<?php
// oracle-probe: id=oracle-builtin-contract-function-msg-queue-exists-2a093072df area=builtin_contract kind=function symbol=msg_queue_exists source=ext/sysvmsg/sysvmsg.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-msg-queue-exists-2a093072df failure_category=builtin_contract requires_ref_extension=sysvmsg
$name = "msg_queue_exists";
echo function_exists($name) ? "available\n" : "missing\n";
