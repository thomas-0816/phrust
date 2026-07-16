<?php

final class LazyCrossUnitNestedMethodHook {
    public array $callbacks = array(
        0 => array(
            array(
                'function' => 'lazy_cross_unit_nested_method_callback',
                'accepted_args' => 1,
            ),
        ),
        1 => array(
            array(
                'function' => 'lazy_cross_unit_nested_method_callback',
                'accepted_args' => 1,
            ),
        ),
        10 => array(
            array(
                'function' => 'lazy_cross_unit_nested_method_callback',
                'accepted_args' => 1,
            ),
            array(
                'function' => 'lazy_cross_unit_nested_method_callback',
                'accepted_args' => 1,
            ),
        ),
    );
    public array $priorities = array(0, 1, 10);
    public array $iterations = array();
    public array $current_priority = array();
    public bool $doing_action = false;
    public int $nesting_level = 0;

    public function apply_filters($value, $args) {
        if (!$this->callbacks) {
            return $value;
        }

        $nesting_level = $this->nesting_level++;
        $this->iterations[$nesting_level] = $this->priorities;
        $num_args = count($args);

        do {
            $this->current_priority[$nesting_level] = current($this->iterations[$nesting_level]);
            $priority = $this->current_priority[$nesting_level];

            foreach ($this->callbacks[$priority] as $callback) {
                if (!$this->doing_action) {
                    $args[0] = $value;
                }
                if (0 === $callback['accepted_args']) {
                    $value = call_user_func($callback['function']);
                } elseif ($callback['accepted_args'] >= $num_args) {
                    $value = call_user_func_array($callback['function'], $args);
                } else {
                    $value = call_user_func_array(
                        $callback['function'],
                        array_slice($args, 0, $callback['accepted_args'])
                    );
                }
            }
        } while (false !== next($this->iterations[$nesting_level]));

        unset($this->iterations[$nesting_level]);
        unset($this->current_priority[$nesting_level]);
        --$this->nesting_level;
        return $value;
    }

    public function do_action($args) {
        $this->doing_action = true;
        $this->apply_filters('', $args);
        if (!$this->nesting_level) {
            $this->doing_action = false;
        }
        return $args[0];
    }

    public function throw_after_replacing_value($value, $args) {
        $value = null;
        throw new RuntimeException('large-unit trace survives released parameter');
    }

    public function invoke_throwing($args) {
        $this->throw_after_replacing_value('', $args);
    }

    // Keep this dynamic unit above the small direct-call graph threshold so
    // do_action() reaches apply_filters() through the production native call
    // trampoline used by large framework classes.
    private function unused00() {}
    private function unused01() {}
    private function unused02() {}
    private function unused03() {}
    private function unused04() {}
    private function unused05() {}
    private function unused06() {}
    private function unused07() {}
    private function unused08() {}
    private function unused09() {}
    private function unused10() {}
    private function unused11() {}
    private function unused12() {}
    private function unused13() {}
}
