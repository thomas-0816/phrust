<?php
// runtime-semantics: expect=pass
eval('const EVAL_REDECLARED_SYMBOL_CONST = "first";');
eval('const EVAL_REDECLARED_SYMBOL_CONST = "second";');
