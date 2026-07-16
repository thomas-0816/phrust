<?php
// oracle-probe: id=oracle-callable-dispatch-function-call-user-func-array-7964d8c027 area=callable_dispatch kind=function symbol=call-user-func-array source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-callable-dispatch-function-call-user-func-array-7964d8c027 failure_category=callable_dispatch
echo call_user_func_array("strlen", ["abc"]), "\n";
