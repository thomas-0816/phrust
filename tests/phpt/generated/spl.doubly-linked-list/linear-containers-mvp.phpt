--TEST--
SPL generated linear container MVP covers SplDoublyLinkedList, SplStack, and SplQueue
--FILE--
<?php
$list = new SplDoublyLinkedList();
$list->push('a');
$list->push('b');
echo $list->top(), '|', $list->bottom(), '|', count($list), "\n";
echo $list->pop(), '|', $list->shift(), '|', count($list), "\n";

$stack = new SplStack();
$stack->push('x');
$stack->push('y');
echo $stack->pop(), '|', $stack->pop(), "\n";

$queue = new SplQueue();
$queue->push(3);
$queue->push(4);
foreach ($queue as $key => $value) {
    echo "$key=$value\n";
}
echo ($queue instanceof SplDoublyLinkedList) ? "list\n" : "not-list\n";
?>
--EXPECT--
b|a|2
b|a|0
y|x
0=3
1=4
list
