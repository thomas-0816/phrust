<?php
var_dump(define("LOCAL_DYNAMIC", "ok"));
var_dump(defined("LOCAL_DYNAMIC"));
echo LOCAL_DYNAMIC, "|", constant("LOCAL_DYNAMIC"), "|";
var_dump(define("LOCAL_DYNAMIC", "again"));
