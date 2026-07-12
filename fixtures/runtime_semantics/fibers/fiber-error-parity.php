<?php
try {
    Fiber::suspend(1);
} catch (FiberError $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
$f = new Fiber(function() { return 42; });
$f->start();
try { $f->start(); } catch (FiberError $e) { echo $e->getMessage(), "\n"; }
var_dump($f->getReturn());
echo "done\n";
$f = new Fiber(function() { Fiber::suspend(); });
$f->start();
try { $f->resume(); $f->resume(); } catch (FiberError $e) { echo $e->getMessage(), "\n"; }
$f = new Fiber(function() { Fiber::suspend(); });
try { $f->throw(new Exception("x")); } catch (FiberError $e) { echo $e->getMessage(), "\n"; }
$g = new Fiber(function() { return 1; });
try { $g->getReturn(); } catch (FiberError $e) { echo $e->getMessage(), "\n"; }
