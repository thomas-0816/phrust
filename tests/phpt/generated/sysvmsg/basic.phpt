--TEST--
sysvmsg host System V message queue compatibility slice
--EXTENSIONS--
sysvmsg
--FILE--
<?php
echo extension_loaded('sysvmsg') ? "loaded\n" : "missing\n";
echo function_exists('msg_get_queue') ? "function\n" : "no function\n";
echo class_exists('SysvMessageQueue') ? "class\n" : "no class\n";
var_dump(defined('MSG_ENOMSG'));

$keyFile = tempnam(sys_get_temp_dir(), "phrust-msg-");
$key = ftok($keyFile, "M");
var_dump(msg_queue_exists($key));
$queue = msg_get_queue($key, 0600);
var_dump($queue instanceof SysvMessageQueue);
var_dump(msg_queue_exists($key));

$error = -1;
var_dump(msg_send($queue, 7, ['name' => 'phrust'], true, true, $error));
var_dump($error);

$type = 0;
$message = null;
$error = -1;
var_dump(msg_receive($queue, 0, $type, 4096, $message, true, 0, $error));
var_dump($type);
var_dump($message);
var_dump($error);

var_dump(msg_send($queue, 9, 'raw', false));
$type = 0;
$message = null;
var_dump(msg_receive($queue, 9, $type, 4096, $message, false));
var_dump($type);
var_dump($message);

$stats = msg_stat_queue($queue);
var_dump(isset($stats['msg_qnum']));
var_dump(msg_set_queue($queue, ['msg_qbytes' => 64, 'msg_perm.mode' => 0600]));
var_dump(msg_remove_queue($queue));
var_dump(msg_queue_exists($key));
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
bool(true)
int(0)
bool(true)
int(7)
array(1) {
  ["name"]=>
  string(6) "phrust"
}
int(0)
bool(true)
bool(true)
int(9)
string(3) "raw"
bool(true)
bool(true)
bool(true)
bool(false)
