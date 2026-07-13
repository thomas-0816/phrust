--TEST--
sockets loopback TCP and Unix-domain basics
--SKIPIF--
<?php
if (!extension_loaded("sockets")) die("skip sockets extension not loaded");
if (!defined("AF_UNIX")) die("skip AF_UNIX not available");
?>
--FILE--
<?php
$server = socket_create(AF_INET, SOCK_STREAM, SOL_TCP);
echo get_class($server), "\n";
var_dump(socket_set_option($server, SOL_SOCKET, SO_REUSEADDR, 1));
var_dump(socket_get_option($server, SOL_SOCKET, SO_REUSEADDR));
var_dump(socket_bind($server, "127.0.0.1", 0));
var_dump(socket_listen($server, 1));
$addr = null;
$port = null;
var_dump(socket_getsockname($server, $addr, $port));
echo $addr, "\n";
echo is_int($port) && $port > 0 ? "port\n" : "no-port\n";

$client = socket_create(AF_INET, SOCK_STREAM, SOL_TCP);
var_dump(socket_connect($client, "127.0.0.1", $port));
var_dump(socket_set_option($client, SOL_TCP, TCP_NODELAY, true));
var_dump(socket_get_option($client, SOL_TCP, TCP_NODELAY));
$accepted = socket_accept($server);
echo get_class($accepted), "\n";

var_dump(socket_write($client, "ping"));
echo socket_read($accepted, 4, PHP_BINARY_READ), "\n";
var_dump(socket_getpeername($client, $peerAddr, $peerPort));
echo $peerAddr, "\n";
echo is_int($peerPort) && $peerPort > 0 ? "peer-port\n" : "no-peer-port\n";
var_dump(socket_send($accepted, "pong", 4, 0));
$buffer = "";
var_dump(socket_recv($client, $buffer, 4, 0));
echo $buffer, "\n";
var_dump(bin2hex(inet_pton("127.0.0.1")));
var_dump(inet_ntop(inet_pton("127.0.0.1")));
var_dump(socket_shutdown($client, SHUT_RDWR));

socket_close($accepted);
socket_close($client);
socket_close($server);

$path = "/tmp/phrust-sockets-basic.sock";
@unlink($path);
$unixServer = socket_create(AF_UNIX, SOCK_STREAM, 0);
echo get_class($unixServer), "\n";
var_dump(socket_bind($unixServer, $path));
var_dump(socket_listen($unixServer, 1));
$unixClient = socket_create(AF_UNIX, SOCK_STREAM, 0);
var_dump(socket_connect($unixClient, $path));
$unixAccepted = socket_accept($unixServer);
echo get_class($unixAccepted), "\n";
var_dump(socket_write($unixClient, "ux"));
echo socket_read($unixAccepted, 2, PHP_BINARY_READ), "\n";
var_dump(socket_getsockname($unixServer, $unixName));
echo $unixName === $path ? "unix-name\n" : "unix-name-mismatch\n";
var_dump(socket_send($unixAccepted, "ok", 2, 0));
$unixBuffer = "";
var_dump(socket_recv($unixClient, $unixBuffer, 2, 0));
echo $unixBuffer, "\n";
socket_close($unixAccepted);
socket_close($unixClient);
socket_close($unixServer);
@unlink($path);

var_dump(socket_create(999999, 999999, 999999));
echo is_string(socket_strerror(socket_last_error())) ? "error-string\n" : "not-string\n";
socket_clear_error();
var_dump(socket_last_error());
?>
--EXPECT--
Socket
bool(true)
int(1)
bool(true)
bool(true)
bool(true)
127.0.0.1
port
bool(true)
bool(true)
int(1)
Socket
int(4)
ping
bool(true)
127.0.0.1
peer-port
int(4)
int(4)
pong
string(8) "7f000001"
string(9) "127.0.0.1"
bool(true)
Socket
bool(true)
bool(true)
bool(true)
Socket
int(2)
ux
bool(true)
unix-name
int(2)
int(2)
ok
bool(false)
error-string
int(0)
