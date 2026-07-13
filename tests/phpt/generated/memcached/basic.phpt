--TEST--
memcached endpoint-backed client fails closed without a configured daemon
--SKIPIF--
<?php if (!extension_loaded("memcached")) die("skip memcached extension not loaded"); ?>
--FILE--
<?php
var_dump(extension_loaded("memcached"));
var_dump(class_exists("Memcached", false));
$m = new Memcached();
var_dump($m instanceof Memcached);
var_dump(method_exists($m, "getMulti"));
var_dump(Memcached::RES_SUCCESS);
var_dump(Memcached::RES_NOTFOUND);
var_dump(Memcached::RES_FAILURE);
var_dump($m->addServer("127.0.0.1", 1));
var_dump($m->getResultCode());
var_dump($m->getResultMessage());
var_dump(count($m->getServerList()));
var_dump($m->set("alpha", "one"));
var_dump($m->get("alpha"));
var_dump($m->delete("alpha"));
var_dump($m->add("alpha", "two"));
var_dump($m->replace("alpha", "three"));
var_dump($m->setMulti(["beta" => "two", "gamma" => "three"]));
var_dump($m->getMulti(["alpha", "beta", "missing"]));
var_dump($m->increment("count", 2, 10));
var_dump($m->decrement("count", 3));
var_dump($m->getResultCode());
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
int(0)
int(16)
int(1)
bool(false)
int(1)
string(7) "FAILURE"
int(0)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
int(1)
