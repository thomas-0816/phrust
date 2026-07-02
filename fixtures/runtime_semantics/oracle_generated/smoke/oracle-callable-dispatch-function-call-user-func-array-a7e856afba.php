<?php
// oracle-probe: id=oracle-callable-dispatch-function-call-user-func-array-a7e856afba area=callable_dispatch kind=function symbol=call-user-func-array source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-callable-dispatch-function-call-user-func-array-a7e856afba failure_category=callable_dispatch
echo call_user_func_array("strlen", ["abc"]), "\n";
