--TEST--
shmop host SysV shared-memory compatibility slice
--EXTENSIONS--
shmop
--FILE--
<?php
echo extension_loaded('shmop') ? "loaded\n" : "missing\n";
echo function_exists('shmop_open') ? "function\n" : "no function\n";
echo class_exists('Shmop') ? "class\n" : "no class\n";

$keyFile = tempnam(sys_get_temp_dir(), "phrust-shmop-");
$key = ftok($keyFile, "S");
$shm = shmop_open($key, "n", 0600, 16);
var_dump($shm instanceof Shmop);
echo shmop_size($shm), "\n";
echo shmop_write($shm, "abc\0def", 0), "\n";
echo bin2hex(shmop_read($shm, 0, 7)), "\n";

$same = shmop_open($key, "a", 0600, 0);
echo bin2hex(shmop_read($same, 0, 7)), "\n";
try {
    shmop_write($same, "x", 0);
} catch (Throwable $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}

$privateA = shmop_open(0, "c", 0600, 4);
$privateB = shmop_open(0, "c", 0600, 4);
shmop_write($privateA, "test", 0);
var_dump(shmop_read($privateB, 0, 4) === "test");
var_dump(shmop_delete($shm));
@unlink($keyFile);
?>
--EXPECT--
loaded
function
class
bool(true)
16
7
61626300646566
61626300646566
Error: Read-only segment cannot be written
bool(false)
bool(true)
