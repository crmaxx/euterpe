//! `tracing` integration: ERROR events → Hawk, INFO/WARN → breadcrumbs.

use std::fmt::Write as _;
use std::sync::Arc;

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use crate::catcher::{CatchOpts, Hawk};
use crate::event::EventLevel;
use crate::scope;

/// Sends `tracing::ERROR` records to Hawk; lower levels become breadcrumbs in scope.
pub struct HawkLayer {
    hawk: Arc<Hawk>,
}

impl HawkLayer {
    pub fn new(hawk: Arc<Hawk>) -> Self {
        Self { hawk }
    }
}

impl<S> Layer<S> for HawkLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);
        let target = meta.target().to_string();
        let message = visitor.message.unwrap_or_else(|| meta.name().to_string());

        match *meta.level() {
            Level::ERROR => {
                self.hawk.catch_message(
                    message,
                    "TracingError",
                    CatchOpts {
                        level: EventLevel::Error,
                        context: visitor.fields,
                        ..Default::default()
                    },
                );
            }
            Level::WARN | Level::INFO => {
                scope::add_breadcrumb(meta.level().as_str(), message, target);
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
    fields: Option<serde_json::Value>,
}

impl Visit for EventVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}").trim_matches('"').to_string());
        } else {
            let mut buf = String::new();
            let _ = write!(buf, "{value:?}");
            self.push_field(field.name(), buf.trim_matches('"').to_string());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.push_field(field.name(), value.to_string());
        }
    }
}

impl EventVisitor {
    fn push_field(&mut self, name: &str, value: String) {
        let map = match &mut self.fields {
            Some(serde_json::Value::Object(m)) => m,
            _ => {
                self.fields = Some(serde_json::json!({}));
                self.fields.as_mut().unwrap().as_object_mut().unwrap()
            }
        };
        map.insert(name.to_string(), serde_json::Value::String(value));
    }
}
