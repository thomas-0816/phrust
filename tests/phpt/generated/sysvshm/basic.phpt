--TEST--
sysvshm host System V shared variable compatibility slice
--EXTENSIONS--
sysvshm
--FILE--
<?php
echo extension_loaded('sysvshm') ? "loaded\n" : "missing\n";
echo function_exists('shm_attach') ? "function\n" : "no function\n";
echo class_exists('SysvSharedMemory') ? "class\n" : "no class\n";

$keyFile = tempnam(sys_get_temp_dir(), "phrust-shm-");
$key = ftok($keyFile, "S");
$shm = shm_attach($key, 1024, 0600);
var_dump($shm instanceof SysvSharedMemory);
var_dump(shm_has_var($shm, 10));
var_dump(shm_put_var($shm, 10, ['value' => 42]));
var_dump(shm_has_var($shm, 10));
var_dump(shm_get_var($shm, 10));
var_dump(shm_remove_var($shm, 10));
var_dump(shm_has_var($shm, 10));
var_dump(shm_remove($shm));
var_dump(shm_detach($shm));
try {
    shm_remove($shm);
} catch (Error $exception) {
    echo $exception->getMessage(), "\n";
}
@unlink($keyFile);
?>
--EXPECT--
loaded
function
class
bool(true)
bool(false)
bool(true)
bool(true)
array(1) {
  ["value"]=>
  int(42)
}
bool(true)
bool(false)
bool(true)
bool(true)
Shared memory block has already been destroyed
