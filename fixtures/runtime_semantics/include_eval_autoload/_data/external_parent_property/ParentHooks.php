<?php
namespace Vendor\Requests;

class ParentHooks
{
    protected $hooks = [];

    public function register(string $name, callable $callback, int $priority = 0): void
    {
        if (!isset($this->hooks[$name])) {
            $this->hooks[$name] = [
                $priority => [],
            ];
        } elseif (!isset($this->hooks[$name][$priority])) {
            $this->hooks[$name][$priority] = [];
        }

        $this->hooks[$name][$priority][] = $callback;
    }

    public function dispatch(string $name, array $parameters = []): bool
    {
        if (empty($this->hooks[$name])) {
            return false;
        }
        ksort($this->hooks[$name]);
        foreach ($this->hooks[$name] as $callbacks) {
            foreach ($callbacks as $callback) {
                $callback(...$parameters);
            }
        }
        return true;
    }
}
