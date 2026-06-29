<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

class AppFlowLogger {
    public function tag($value) {
        return 'log-' . $value;
    }
}

class AppFlowRepository {
    private $logger;

    public function __construct($logger) {
        $this->logger = $logger;
    }

    public function findLabel($id) {
        return $this->logger->tag('row-' . $id);
    }
}

class AppFlowContainer {
    private $loggerFactory = '';
    private $repositoryFactory = '';
    private $loggerSingleton = null;
    private $repositorySingleton = null;

    public function set($name, $factory) {
        if ($name === 'logger') {
            $this->loggerFactory = $factory;
        } else {
            $this->repositoryFactory = $factory;
        }
    }

    public function get($name) {
        if ($name === 'logger') {
            if ($this->loggerSingleton !== null) {
                return $this->loggerSingleton;
            }
            $service = app_flow_logger_factory($this);
            $this->loggerSingleton = $service;
            return $service;
        }
        if ($this->repositorySingleton !== null) {
            return $this->repositorySingleton;
        }
        $service = app_flow_repository_factory($this);
        $this->repositorySingleton = $service;
        return $service;
    }
}

function app_flow_logger_factory($container) {
    return new AppFlowLogger();
}

function app_flow_repository_factory($container) {
    return new AppFlowRepository($container->get('logger'));
}

$container = new AppFlowContainer();
$container->set('logger', 'logger_factory');
$container->set('repository', 'repository_factory');

$checksum = 0;
$items = 0;
for ($i = 0; $i < app_flow_scale() * 40; $i++) {
    $repo = $container->get('repository');
    $label = $repo->findLabel($i % 17);
    $checksum = $checksum + strlen($label) + ($i % 5);
    $items++;
}
echo 'app-flow dependency_container_resolution checksum=' . $checksum . ' items=' . $items . "\n";
