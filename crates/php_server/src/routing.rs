use std::{
    fs::Metadata,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteConfig {
    pub docroot: PathBuf,
    pub index: String,
    pub front_controller: Option<PathBuf>,
    pub builtin_router: Option<PathBuf>,
    pub metrics_endpoint_enabled: bool,
    pub cache_clear_endpoint_enabled: bool,
}

#[derive(Clone, Debug)]
pub enum ResolvedRoute {
    Health,
    Metrics,
    CacheClear,
    StaticFile {
        path: PathBuf,
        metadata: Metadata,
    },
    PhpScript {
        script_path: PathBuf,
        path_info: Option<String>,
    },
    NotFound,
    Forbidden,
    BadRequest,
    MethodNotAllowed,
}

pub fn resolve_route(method: &str, path: &str, config: &RouteConfig) -> ResolvedRoute {
    if path == "/healthz" {
        return ResolvedRoute::Health;
    }
    if path == "/__phrust/metrics" {
        if !config.metrics_endpoint_enabled {
            return ResolvedRoute::NotFound;
        }
        return if method == "GET" {
            ResolvedRoute::Metrics
        } else {
            ResolvedRoute::MethodNotAllowed
        };
    }
    if path == "/__phrust/cache/clear" {
        if !config.cache_clear_endpoint_enabled {
            return ResolvedRoute::NotFound;
        }
        return if method == "POST" {
            ResolvedRoute::CacheClear
        } else {
            ResolvedRoute::MethodNotAllowed
        };
    }
    let Some(relative_segments) = decoded_relative_segments(path) else {
        return ResolvedRoute::BadRequest;
    };
    let relative_path = relative_path_from_segments(&relative_segments);
    if contains_forbidden_component(&relative_path) {
        return ResolvedRoute::Forbidden;
    }

    let candidate = config.docroot.join(&relative_path);
    if candidate.exists() {
        return resolve_existing_path(method, &candidate, None, config);
    }
    if is_missing_browser_static_probe(&relative_path) {
        return ResolvedRoute::NotFound;
    }
    if let Some(route) = resolve_path_info_script(method, &relative_segments, config) {
        return route;
    }

    if let Some(front_controller) = &config.front_controller {
        let script_path = config.docroot.join(front_controller);
        if !script_path.exists() {
            return ResolvedRoute::NotFound;
        }
        return resolve_existing_path(method, &script_path, path_info(path), config);
    }

    ResolvedRoute::NotFound
}

fn is_missing_browser_static_probe(path: &Path) -> bool {
    path == Path::new("favicon.ico")
}

fn resolve_path_info_script(
    method: &str,
    relative_segments: &[String],
    config: &RouteConfig,
) -> Option<ResolvedRoute> {
    for split in 1..relative_segments.len() {
        let script_relative = relative_path_from_segments(&relative_segments[..split]);
        if !is_php_path(&script_relative) {
            continue;
        }
        let script_path = config.docroot.join(&script_relative);
        if !script_path.exists() {
            continue;
        }
        let path_info = format!("/{}", relative_segments[split..].join("/"));
        return Some(resolve_existing_path(
            method,
            &script_path,
            Some(path_info),
            config,
        ));
    }
    None
}

fn resolve_existing_path(
    method: &str,
    candidate: &Path,
    path_info: Option<String>,
    config: &RouteConfig,
) -> ResolvedRoute {
    let Ok(canonical) = candidate.canonicalize() else {
        return ResolvedRoute::NotFound;
    };
    if !canonical.starts_with(&config.docroot) {
        return ResolvedRoute::Forbidden;
    }
    let Ok(metadata) = canonical.metadata() else {
        return ResolvedRoute::NotFound;
    };
    if metadata.is_dir() {
        let index = canonical.join(&config.index);
        if index.exists() {
            return resolve_existing_path(method, &index, path_info, config);
        }
        return ResolvedRoute::Forbidden;
    }
    if is_php_path(&canonical) {
        if !is_php_application_method(method) {
            return ResolvedRoute::MethodNotAllowed;
        }
        return ResolvedRoute::PhpScript {
            script_path: canonical,
            path_info,
        };
    }
    if method != "GET" && method != "HEAD" {
        return ResolvedRoute::MethodNotAllowed;
    }
    ResolvedRoute::StaticFile {
        path: canonical,
        metadata,
    }
}

fn decoded_relative_segments(path: &str) -> Option<Vec<String>> {
    if !path.starts_with('/') {
        return None;
    }
    let decoded = percent_decode(path.as_bytes())?;
    if decoded.contains(&0) {
        return None;
    }
    let decoded = String::from_utf8(decoded).ok()?;
    let without_root = decoded.strip_prefix('/')?;
    if without_root.starts_with('/') {
        return None;
    }
    Some(
        without_root
            .split('/')
            .filter(|segment| !segment.is_empty() && *segment != ".")
            .map(str::to_string)
            .collect(),
    )
}

fn relative_path_from_segments(segments: &[String]) -> PathBuf {
    let mut relative = PathBuf::new();
    for segment in segments {
        relative.push(segment);
    }
    relative
}

fn percent_decode(input: &[u8]) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == b'%' {
            let high = *input.get(index + 1)?;
            let low = *input.get(index + 2)?;
            output.push(hex_value(high)? << 4 | hex_value(low)?);
            index += 3;
        } else {
            output.push(input[index]);
            index += 1;
        }
    }
    Some(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn contains_forbidden_component(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    })
}

fn is_php_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("php"))
}

fn is_php_application_method(method: &str) -> bool {
    matches!(
        method,
        "GET" | "HEAD" | "POST" | "OPTIONS" | "PUT" | "PATCH" | "DELETE"
    )
}

fn path_info(path: &str) -> Option<String> {
    if path == "/" {
        None
    } else {
        Some(path.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{ResolvedRoute, RouteConfig, resolve_route};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn rejects_parent_traversal() {
        let fixture = Fixture::new();

        assert!(matches!(
            resolve_route("GET", "/../secret", &fixture.config("index.html")),
            ResolvedRoute::Forbidden
        ));
    }

    #[test]
    fn rejects_encoded_parent_traversal() {
        let fixture = Fixture::new();

        assert!(matches!(
            resolve_route("GET", "/%2e%2e/secret", &fixture.config("index.html")),
            ResolvedRoute::Forbidden
        ));
    }

    #[test]
    fn rejects_nul_path() {
        let fixture = Fixture::new();

        assert!(matches!(
            resolve_route("GET", "/bad%00path", &fixture.config("index.html")),
            ResolvedRoute::BadRequest
        ));
    }

    #[test]
    fn rejects_malformed_percent_escape() {
        let fixture = Fixture::new();

        assert!(matches!(
            resolve_route("GET", "/bad%xxpath", &fixture.config("index.html")),
            ResolvedRoute::BadRequest
        ));
    }

    #[test]
    fn rejects_encoded_absolute_path_attempt() {
        let fixture = Fixture::new();

        assert!(matches!(
            resolve_route("GET", "/%2fetc/passwd", &fixture.config("index.html")),
            ResolvedRoute::BadRequest
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_from_docroot() {
        let fixture = Fixture::new();
        let outside = fixture.root.with_extension("outside");
        fs::write(&outside, "secret").expect("write outside file");
        std::os::unix::fs::symlink(&outside, fixture.root.join("link.txt"))
            .expect("create symlink");

        assert!(matches!(
            resolve_route("GET", "/link.txt", &fixture.config("index.html")),
            ResolvedRoute::Forbidden
        ));

        let _ = fs::remove_file(outside);
    }

    #[test]
    fn maps_static_file() {
        let fixture = Fixture::new();
        fixture.write("static.txt", "static\n");

        assert!(matches!(
            resolve_route("GET", "/static.txt", &fixture.config("index.html")),
            ResolvedRoute::StaticFile { .. }
        ));
    }

    #[test]
    fn maps_php_script() {
        let fixture = Fixture::new();
        fixture.write("hello.php", "<?php echo \"hi\";");

        assert!(matches!(
            resolve_route("GET", "/hello.php", &fixture.config("index.html")),
            ResolvedRoute::PhpScript { .. }
        ));
    }

    #[test]
    fn maps_options_to_php_script() {
        let fixture = Fixture::new();
        fixture.write("index.php", "<?php echo \"front\";");
        let mut config = fixture.config("index.php");
        config.front_controller = Some(PathBuf::from("index.php"));

        assert!(matches!(
            resolve_route("OPTIONS", "/index.php", &config),
            ResolvedRoute::PhpScript { .. }
        ));
        assert!(matches!(
            resolve_route("OPTIONS", "/wp-json/wp/v2/settings", &config),
            ResolvedRoute::PhpScript { .. }
        ));
    }

    #[test]
    fn maps_rest_write_methods_to_php_script() {
        let fixture = Fixture::new();
        fixture.write("index.php", "<?php echo \"front\";");
        let mut config = fixture.config("index.php");
        config.front_controller = Some(PathBuf::from("index.php"));

        for method in ["PUT", "PATCH", "DELETE"] {
            assert!(
                matches!(
                    resolve_route(method, "/index.php", &config),
                    ResolvedRoute::PhpScript { .. }
                ),
                "{method} should route existing PHP scripts"
            );
            assert!(
                matches!(
                    resolve_route(method, "/wp-json/wp/v2/posts/1", &config),
                    ResolvedRoute::PhpScript { .. }
                ),
                "{method} should route front-controller REST paths"
            );
        }
    }

    #[test]
    fn rejects_write_methods_for_static_files() {
        let fixture = Fixture::new();
        fixture.write("static.txt", "hello");

        for method in ["POST", "PUT", "PATCH", "DELETE", "OPTIONS"] {
            assert!(
                matches!(
                    resolve_route(method, "/static.txt", &fixture.config("index.html")),
                    ResolvedRoute::MethodNotAllowed
                ),
                "{method} should not route static files"
            );
        }
    }

    #[test]
    fn maps_metrics_endpoint_when_enabled() {
        let fixture = Fixture::new();

        assert!(matches!(
            resolve_route("GET", "/__phrust/metrics", &fixture.config("index.html")),
            ResolvedRoute::Metrics
        ));
    }

    #[test]
    fn hides_metrics_endpoint_when_disabled() {
        let fixture = Fixture::new();
        let mut config = fixture.config("index.html");
        config.metrics_endpoint_enabled = false;

        assert!(matches!(
            resolve_route("GET", "/__phrust/metrics", &config),
            ResolvedRoute::NotFound
        ));
    }

    #[test]
    fn hides_cache_clear_endpoint_by_default() {
        let fixture = Fixture::new();

        assert!(matches!(
            resolve_route(
                "POST",
                "/__phrust/cache/clear",
                &fixture.config("index.html")
            ),
            ResolvedRoute::NotFound
        ));
    }

    #[test]
    fn maps_cache_clear_endpoint_when_enabled() {
        let fixture = Fixture::new();
        let mut config = fixture.config("index.html");
        config.cache_clear_endpoint_enabled = true;

        assert!(matches!(
            resolve_route("POST", "/__phrust/cache/clear", &config),
            ResolvedRoute::CacheClear
        ));
        assert!(matches!(
            resolve_route("GET", "/__phrust/cache/clear", &config),
            ResolvedRoute::MethodNotAllowed
        ));
    }

    #[test]
    fn maps_front_controller_for_missing_path() {
        let fixture = Fixture::new();
        fixture.write("index.php", "<?php echo \"front\";");
        let mut config = fixture.config("index.php");
        config.front_controller = Some(PathBuf::from("index.php"));

        let ResolvedRoute::PhpScript { path_info, .. } = resolve_route("GET", "/users/1", &config)
        else {
            panic!("expected front controller script");
        };
        assert_eq!(path_info.as_deref(), Some("/users/1"));
    }

    #[test]
    fn skips_front_controller_for_missing_favicon_probe() {
        let fixture = Fixture::new();
        fixture.write("index.php", "<?php echo \"front\";");
        let mut config = fixture.config("index.php");
        config.front_controller = Some(PathBuf::from("index.php"));

        for path in ["/favicon.ico", "/favicon.ico/"] {
            assert!(
                matches!(resolve_route("GET", path, &config), ResolvedRoute::NotFound),
                "{path} should not route through the front controller"
            );
        }
    }

    #[test]
    fn serves_existing_favicon_probe_as_static_file() {
        let fixture = Fixture::new();
        fixture.write("index.php", "<?php echo \"front\";");
        fixture.write("favicon.ico", "ico");
        let mut config = fixture.config("index.php");
        config.front_controller = Some(PathBuf::from("index.php"));

        assert!(matches!(
            resolve_route("GET", "/favicon.ico", &config),
            ResolvedRoute::StaticFile { .. }
        ));
    }

    #[test]
    fn maps_path_info_after_existing_php_script_before_front_controller() {
        let fixture = Fixture::new();
        fixture.write("index.php", "<?php echo \"index\";");
        fixture.create_dir("wp-admin");
        fixture.write("wp-admin/install.php", "<?php echo \"install\";");
        let mut config = fixture.config("index.php");
        config.front_controller = Some(PathBuf::from("index.php"));

        let ResolvedRoute::PhpScript {
            script_path,
            path_info,
        } = resolve_route("GET", "/index.php/pretty/path", &config)
        else {
            panic!("expected index.php script");
        };
        assert_eq!(
            script_path,
            fixture
                .root
                .join("index.php")
                .canonicalize()
                .expect("canonical index")
        );
        assert_eq!(path_info.as_deref(), Some("/pretty/path"));

        let ResolvedRoute::PhpScript {
            script_path,
            path_info,
        } = resolve_route("GET", "/wp-admin/install.php/step/1", &config)
        else {
            panic!("expected install.php script");
        };
        assert_eq!(
            script_path,
            fixture
                .root
                .join("wp-admin/install.php")
                .canonicalize()
                .expect("canonical install")
        );
        assert_eq!(path_info.as_deref(), Some("/step/1"));
    }

    #[test]
    fn decodes_path_info_segments_for_existing_php_script() {
        let fixture = Fixture::new();
        fixture.write("index.php", "<?php echo \"index\";");

        let ResolvedRoute::PhpScript { path_info, .. } = resolve_route(
            "GET",
            "/index.php/wp%20admin/step",
            &fixture.config("index.php"),
        ) else {
            panic!("expected index.php script");
        };

        assert_eq!(path_info.as_deref(), Some("/wp admin/step"));
    }

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let unique = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("phrust-routing-{}-{unique}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir(&root).expect("create fixture root");
            Self { root }
        }

        fn write(&self, relative: &str, contents: &str) {
            fs::write(self.root.join(relative), contents).expect("write fixture file");
        }

        fn create_dir(&self, relative: &str) {
            fs::create_dir_all(self.root.join(relative)).expect("create fixture dir");
        }

        fn config(&self, index: &str) -> RouteConfig {
            RouteConfig {
                docroot: self.root.canonicalize().expect("canonical docroot"),
                index: index.to_string(),
                front_controller: None,
                builtin_router: None,
                metrics_endpoint_enabled: true,
                cache_clear_endpoint_enabled: false,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
