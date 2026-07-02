<?php
// runtime-semantics: expect=pass
eval('if (false) { function eval_conditional_declared_function() { return "no"; } }');
echo function_exists("eval_conditional_declared_function") ? "declared" : "missing";
eval('if (true) { function eval_conditional_declared_function() { return "conditional"; } }');
echo "|", eval_conditional_declared_function(), "\n";
