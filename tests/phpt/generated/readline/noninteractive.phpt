--TEST--
readline noninteractive history and info compatibility slice
--EXTENSIONS--
readline
--FILE--
<?php
echo extension_loaded('readline') ? "loaded\n" : "missing\n";
echo function_exists('readline_info') ? "function\n" : "no function\n";
echo READLINE_LIB, "\n";
var_dump(readline());
var_dump(readline('Prompt: '));

var_dump(readline_add_history('foo'));
var_dump(readline_add_history(''));
var_dump(readline_list_history());

var_dump(readline_info('line_buffer'));
var_dump(readline_info('line_buffer', 'abc'));
var_dump(readline_info('line_buffer'));

$name = tempnam(sys_get_temp_dir(), 'phrust-readline-');
var_dump(readline_write_history($name));
var_dump(readline_clear_history());
var_dump(readline_read_history($name));
var_dump(readline_list_history());
unlink($name);

var_dump(readline_completion_function('strlen'));
function phrust_readline_handler($line) {}
var_dump(readline_callback_handler_remove());
var_dump(readline_callback_handler_install('> ', 'phrust_readline_handler'));
readline_callback_read_char();
readline_redisplay();
readline_on_new_line();
var_dump(readline_callback_handler_remove());
?>
--EXPECTF--
loaded
function
phrust
bool(false)
bool(false)
bool(true)
bool(true)
array(2) {
  [0]=>
  string(3) "foo"
  [1]=>
  string(0) ""
}
string(0) ""
string(0) ""
string(3) "abc"
bool(true)
bool(true)
bool(true)
array(2) {
  [0]=>
  string(3) "foo"
  [1]=>
  string(0) ""
}
bool(true)
bool(false)
> bool(true)
bool(true)
