--TEST--
memcached deterministic fake backend options and advanced core methods
--SKIPIF--
<?php if (!extension_loaded("memcached")) die("skip memcached extension not loaded"); ?>
--FILE--
<?php
$m = new Memcached("persistent-id");
var_dump(method_exists($m, "deleteMulti"));
var_dump(method_exists($m, "getResultMessage"));
var_dump(Memcached::RES_FAILURE);
var_dump(Memcached::OPT_SERIALIZER);
var_dump(Memcached::SERIALIZER_PHP);
var_dump(Memcached::OPT_COMPRESSION);
var_dump(Memcached::GET_PRESERVE_ORDER);
var_dump($m->addServers([
    ["127.0.0.1", 11211],
    ["cache.local", 11212, 2],
]));
var_dump(count($m->getServerList()));
var_dump($m->setOption(Memcached::OPT_SERIALIZER, Memcached::SERIALIZER_PHP));
var_dump($m->getOption(Memcached::OPT_SERIALIZER));
var_dump($m->setOptions([
    Memcached::OPT_COMPRESSION => false,
    Memcached::OPT_PREFIX_KEY => "wp:",
]));
var_dump($m->getOption(Memcached::OPT_COMPRESSION));
var_dump($m->getOption(Memcached::OPT_PREFIX_KEY));
var_dump($m->set("alpha", "one"));
var_dump($m->prepend("alpha", "zero-"));
var_dump($m->append("alpha", "-two"));
var_dump($m->get("alpha"));
var_dump($m->cas(123, "alpha", "cas-value"));
var_dump($m->get("alpha"));
var_dump($m->setMulti(["beta" => "two", "gamma" => "three"]));
var_dump($m->deleteMulti(["beta", "missing"]));
$many = $m->getMulti(["alpha", "beta", "gamma"]);
var_dump(isset($many["alpha"]), isset($many["beta"]), $many["gamma"]);
var_dump($m->getResultCode());
var_dump($m->getResultMessage());
var_dump($m->get("missing"));
var_dump($m->getResultCode());
var_dump($m->getResultMessage());
var_dump($m->getStats());
var_dump($m->getVersion());
var_dump($m->flush());
var_dump($m->get("alpha"));
var_dump($m->getResultCode());
?>
--EXPECT--
bool(true)
bool(true)
int(1)
int(-1003)
int(1)
int(-1001)
int(1)
bool(true)
int(2)
bool(true)
int(1)
bool(true)
bool(false)
string(3) "wp:"
bool(true)
bool(true)
bool(true)
string(12) "zero-one-two"
bool(true)
string(9) "cas-value"
bool(true)
bool(true)
bool(true)
bool(false)
string(5) "three"
int(0)
string(7) "SUCCESS"
bool(false)
int(16)
string(9) "NOT FOUND"
array(0) {
}
array(0) {
}
bool(true)
bool(false)
int(16)
