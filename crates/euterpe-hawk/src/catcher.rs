use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::panic::{self, PanicHookInfo};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::backtrace::capture_backtrace;
use crate::config::HawkConfig;
use crate::contexts::build_base_context;
use crate::error_chain::format_error_chain;
use crate::event::{AffectedUser, ErrorReport, EventLevel, Payload};
use crate::filter::default_before_send;
use crate::http_addons::panic_mechanism_addon;
use crate::panic_flag::take_panic_already_reported;
use crate::scope::{current_http_addons, current_scope_context, current_scope_user};
use crate::sender::{HawkGuard, SenderHandle};
use crate::VERSION;

static PANIC_HAWK: OnceLock<Weak<Hawk>> = OnceLock::new();
static ANON_USER_ID: OnceLock<String> = OnceLock::new();

type BeforeSendFn = Arc<dyn Fn(&mut ErrorReport) + Send + Sync>;

struct HawkInner {
    config: HawkConfig,
    sender: SenderHandle,
    before_send: BeforeSendFn,
    dedup: Mutex<HashMap<u64, Instant>>,
}

#[derive(Clone)]
pub struct CatchOpts {
    pub context: Option<Value>,
    pub user: Option<AffectedUser>,
    pub release: Option<String>,
    pub addons: Option<Value>,
    pub description: Option<String>,
    pub level: EventLevel,
    pub urgent: bool,
}

/// Hawk error catcher — thread-safe, async delivery to collector.
pub struct Hawk {
    inner: Arc<HawkInner>,
}

impl Hawk {
    pub fn try_new(mut config: HawkConfig) -> Result<Arc<Self>, crate::token::InvalidHawkToken> {
        if config.token.is_empty() {
            return Err(crate::token::InvalidHawkToken);
        }
        if config.collector_endpoint.is_empty() {
            config.collector_endpoint = crate::token::collector_endpoint_from_token(&config.token)?;
        }

        let sender = SenderHandle::spawn(
            config.collector_endpoint.clone(),
            config.batch_max,
            config.batch_interval,
        );

        let before_send: BeforeSendFn = Arc::new(default_before_send);

        Ok(Arc::new(Self {
            inner: Arc::new(HawkInner {
                config,
                sender,
                before_send,
                dedup: Mutex::new(HashMap::new()),
            }),
        }))
    }

    pub fn from_env() -> Option<Arc<Self>> {
        Self::install_from_env(None).0
    }

    /// Initialize from environment; optional default release (e.g. app crate version).
    pub fn install_from_env(
        default_release: Option<&str>,
    ) -> (Option<Arc<Self>>, Option<HawkGuard>) {
        let mut config = match HawkConfig::from_env() {
            Some(c) => c,
            None => return (None, None),
        };
        if config.release.is_none() {
            config.release = default_release.map(str::to_string);
        }
        let hawk = match Self::try_new(config) {
            Ok(h) => h,
            Err(_) => {
                tracing::warn!("hawk: invalid HAWK_TOKEN, catcher disabled");
                return (None, None);
            }
        };
        hawk.init();
        let guard = HawkGuard::new(
            hawk.inner.sender.clone(),
            hawk.inner.config.flush_timeout,
        );
        (Some(hawk), Some(guard))
    }

    pub fn with_before_send(
        self: Arc<Self>,
        hook: impl Fn(&mut ErrorReport) + Send + Sync + 'static,
    ) -> Arc<Self> {
        let user_hook = Arc::new(hook);
        let prev = Arc::clone(&self.inner.before_send);
        Arc::new(Self {
            inner: Arc::new(HawkInner {
                config: self.inner.config.clone(),
                sender: self.inner.sender.clone(),
                before_send: Arc::new(move |event| {
                    prev(event);
                    user_hook(event);
                }),
                dedup: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn init(self: &Arc<Self>) {
        let _ = PANIC_HAWK.set(Arc::downgrade(self));
        let prev = panic::take_hook();
        let weak = Arc::downgrade(self);
        panic::set_hook(Box::new(move |info| {
            if take_panic_already_reported() {
                // Already sent via Axum CatchPanicLayer.
            } else if let Some(hawk) = weak.upgrade() {
                hawk.report_panic(info);
            }
            prev(info);
        }));
    }

    pub fn catch_error(&self, err: &(dyn Error + Send + Sync), mut opts: CatchOpts) {
        if opts.description.is_none() {
            opts.description = format_error_chain(err);
        }
        let title = err.to_string();
        self.send_report(title, "Error".into(), 3, opts);
    }

    pub fn catch<E>(&self, err: E, opts: CatchOpts)
    where
        E: fmt::Display + fmt::Debug,
    {
        let title = err.to_string();
        let event_type = std::any::type_name::<E>()
            .rsplit("::")
            .next()
            .unwrap_or("Error")
            .to_string();
        self.send_report(title, event_type, 3, opts);
    }

    pub fn catch_message(
        &self,
        title: impl Into<String>,
        event_type: impl Into<String>,
        opts: CatchOpts,
    ) {
        self.send_report(title.into(), event_type.into(), 3, opts);
    }

    pub async fn flush(&self, timeout: Duration) {
        self.inner.sender.flush(timeout).await;
    }

    fn report_panic(&self, info: &PanicHookInfo<'_>) {
        let payload = info.payload();
        let title = if let Some(s) = payload.downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.clone()
        } else {
            "panic".to_string()
        };
        let mut addons = current_http_addons().unwrap_or_else(|| json!({}));
        if let Value::Object(ref mut map) = addons {
            if let Value::Object(m) = panic_mechanism_addon() {
                map.extend(m);
            }
        } else {
            addons = panic_mechanism_addon();
        }
        let opts = CatchOpts {
            level: EventLevel::Fatal,
            urgent: true,
            addons: Some(addons),
            ..Default::default()
        };
        self.send_report(title, "panic".to_string(), 4, opts);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let sender = self.inner.sender.clone();
            let timeout = self.inner.config.flush_timeout;
            handle.spawn(async move {
                sender.flush(timeout).await;
            });
        }
    }

    fn send_report(
        &self,
        title: String,
        event_type: String,
        skip_frames: usize,
        mut opts: CatchOpts,
    ) {
        if !self.should_send(&event_type, &title) {
            return;
        }

        if opts.addons.is_none() {
            opts.addons = current_http_addons();
        }

        let base = build_base_context(&self.inner.config);
        let scope_ctx = current_scope_context();
        let mut context = merge_context(
            Some(base),
            merge_context(self.inner.config.context.clone(), opts.context),
        );
        if let Some(sc) = scope_ctx {
            context = merge_context(context, Some(sc));
        }
        normalize_context(&mut context);

        let user = resolve_user(
            self.inner.config.default_user.clone(),
            opts.user.or_else(current_scope_user),
        );

        let backtrace = capture_backtrace(&self.inner.config, skip_frames);
        let mut report = ErrorReport::new(
            self.inner.config.token.clone(),
            Payload {
                title,
                event_type,
                backtrace,
                description: opts.description,
                level: Some(opts.level.as_u16()),
                release: opts.release.or_else(|| self.inner.config.release.clone()),
                user,
                context,
                catcher_version: VERSION.to_string(),
                addons: opts.addons,
            },
        );

        (self.inner.before_send)(&mut report);
        self.inner
            .sender
            .send(report, opts.urgent);
    }

    fn should_send(&self, event_type: &str, title: &str) -> bool {
        if self.inner.config.sample_rate < 1.0 {
            let mut hasher = DefaultHasher::new();
            title.hash(&mut hasher);
            event_type.hash(&mut hasher);
            let bucket = (hasher.finish() % 10_000) as f32 / 10_000.0;
            if bucket >= self.inner.config.sample_rate {
                return false;
            }
        }

        let key = fingerprint(event_type, title);
        let now = Instant::now();
        let mut dedup = self.inner.dedup.lock().expect("hawk dedup lock");
        dedup.retain(|_, t| now.duration_since(*t) < self.inner.config.dedup_window);
        if let Some(prev) = dedup.get(&key) {
            if now.duration_since(*prev) < self.inner.config.dedup_window {
                return false;
            }
        }
        dedup.insert(key, now);
        true
    }

    pub fn build_http_addons(
        method: &str,
        uri: &str,
        headers: serde_json::Map<String, Value>,
    ) -> Value {
        crate::http_addons::build_http_addons(method, uri, headers)
    }
}

impl Default for CatchOpts {
    fn default() -> Self {
        Self {
            context: None,
            user: None,
            release: None,
            addons: None,
            description: None,
            level: EventLevel::Error,
            urgent: false,
        }
    }
}

fn fingerprint(event_type: &str, title: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    event_type.hash(&mut hasher);
    title.hash(&mut hasher);
    hasher.finish()
}

fn merge_context(global: Option<Value>, local: Option<Value>) -> Option<Value> {
    match (global, local) {
        (None, None) => None,
        (Some(g), None) => Some(g),
        (None, Some(l)) => Some(l),
        (Some(Value::Object(mut g)), Some(Value::Object(l))) => {
            for (k, v) in l {
                g.insert(k, v);
            }
            Some(Value::Object(g))
        }
        (Some(g), Some(l)) => Some(json!({ "global": g, "local": l })),
    }
}

fn normalize_context(context: &mut Option<Value>) {
    if let Some(Value::String(s)) = context.clone() {
        *context = Some(json!({ "value": s }));
    }
}

fn resolve_user(
    default: Option<AffectedUser>,
    per_event: Option<AffectedUser>,
) -> Option<AffectedUser> {
    if let Some(u) = per_event.filter(|u| !u.is_empty()) {
        return Some(u);
    }
    if let Some(u) = default.filter(|u| !u.is_empty()) {
        return Some(u);
    }
    Some(AffectedUser {
        id: Some(anonymous_user_id()),
        ..Default::default()
    })
}

fn anonymous_user_id() -> String {
    ANON_USER_ID
        .get_or_init(|| {
            let mut hasher = DefaultHasher::new();
            std::env::var("HOSTNAME")
                .or_else(|_| std::env::var("COMPUTERNAME"))
                .unwrap_or_else(|_| "unknown".into())
                .hash(&mut hasher);
            format!("user-{:016x}", hasher.finish())
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::collector_endpoint_from_token;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SAMPLE_TOKEN: &str = "eyJpbnRlZ3JhdGlvbklkIjoiZGRjZmY4OTItODMzMy00YjVlLWIyYWQtZWM1MDQ5MDVjMjFlIiwic2VjcmV0IjoiZmJjYzIwMTEtMTY5My00NDIyLThiNDItZDRlMzdlYmI4NWIwIn0=";

    fn sample_config(endpoint: &str) -> HawkConfig {
        HawkConfig {
            token: SAMPLE_TOKEN.to_string(),
            collector_endpoint: endpoint.to_string(),
            release: Some("test-release".into()),
            environment: Some("test".into()),
            context: None,
            source_code_enabled: true,
            source_code_lines: 1,
            backtrace_trim: true,
            default_user: None,
            batch_max: 1,
            batch_interval: Duration::from_millis(50),
            sample_rate: 1.0,
            flush_timeout: Duration::from_secs(2),
            dedup_window: Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn sends_event_to_collector() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let hawk = Hawk::try_new(sample_config(&server.uri())).unwrap();
        hawk.catch_message("sample error", "ManualError", CatchOpts::default());

        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(!server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn flush_on_guard_drop() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let hawk = Hawk::try_new(sample_config(&server.uri())).unwrap();
        hawk.catch_message("flush me", "TestError", CatchOpts::default());
        let guard = HawkGuard::new(hawk.inner.sender.clone(), Duration::from_secs(2));
        guard.flush().await;
        drop(guard);
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn dedup_suppresses_duplicate_within_window() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let hawk = Hawk::try_new(sample_config(&server.uri())).unwrap();
        hawk.catch_message("same", "DupError", CatchOpts::default());
        hawk.catch_message("same", "DupError", CatchOpts::default());
        tokio::time::sleep(Duration::from_millis(150)).await;
        let reqs = server.received_requests().await.unwrap();
        assert_eq!(reqs.len(), 1);
    }

    #[test]
    fn empty_token_is_rejected() {
        let mut config = sample_config("http://localhost");
        config.token.clear();
        assert!(Hawk::try_new(config).is_err());
    }

    #[test]
    fn token_parsing_matches_python_fixture() {
        let url = collector_endpoint_from_token(SAMPLE_TOKEN).unwrap();
        assert_eq!(
            url,
            "https://ddcff892-8333-4b5e-b2ad-ec504905c21e.k1.hawk.so"
        );
    }

    #[tokio::test]
    async fn before_send_strips_sensitive_context() {
        let server = MockServer::start().await;
        let captured = Arc::new(Mutex::new(None));
        let captured_clone = Arc::clone(&captured);

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let hawk = Hawk::try_new(sample_config(&server.uri()))
            .unwrap()
            .with_before_send(move |event| {
                *captured_clone.lock().unwrap() = Some(event.clone());
            });

        hawk.catch_message(
            "err",
            "TestError",
            CatchOpts {
                context: Some(json!({
                    "ping": "pong",
                    "very_sensitive_info": "secret",
                    "password": "x"
                })),
                ..Default::default()
            },
        );

        tokio::time::sleep(Duration::from_millis(150)).await;

        let event = captured.lock().unwrap().clone().expect("event sent");
        let ctx = event.payload.context.expect("context");
        assert_eq!(ctx["ping"], "pong");
        assert_eq!(ctx["very_sensitive_info"], "secret");
        assert!(ctx.get("password").is_none());
        assert_eq!(event.payload.release.as_deref(), Some("test-release"));
        assert_eq!(event.payload.catcher_version, VERSION);
        assert_eq!(event.catcher_type, crate::CATCHER_TYPE);
    }
}
