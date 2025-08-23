use crate::components::level::Level;
use tracing::{Level as TracingLevel};

use std::fmt;
use tracing::{Event, Subscriber, field::{Field, Visit}};
use tracing_subscriber::{layer::{Context as TContext, Layer}, registry::LookupSpan};
use crate::components::context::Context;
use crate::components::span::Span;
use crate::crates::bridge::HtraceBridge;
use crate::htracer::HTracer;

struct Visitor {
	message: Option<String>,
	fields: Vec<(String, String)>,
}
impl Visitor {
	fn new() -> Self { Self { message: None, fields: Vec::new() } }
}
impl Visit for Visitor {
	fn record_str(&mut self, field: &Field, value: &str) {
		if field.name() == "message" {
			self.message = Some(value.to_owned());
		} else {
			self.fields.push((field.name().to_string(), value.to_owned()));
		}
	}
	fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
		if field.name() == "message" {
			self.message = Some(format!("{value:?}"));
		} else {
			self.fields.push((field.name().to_string(), format!("{value:?}")));
		}
	}
}

impl<S> Layer<S> for HtraceBridge
where
	S: Subscriber + for<'a> LookupSpan<'a>,
{
	fn on_event(&self, event: &Event<'_>, ctx: TContext<'_, S>) {
		let mut v = Visitor::new();
		event.record(&mut v);

		// message + champs
		let mut msg = v.message.unwrap_or_else(|| "<no message>".to_string());
		if !v.fields.is_empty() {
			use std::fmt::Write;
			msg.push_str(" {");
			for (i, (k, val)) in v.fields.iter().enumerate() {
				if i > 0 { msg.push_str(", "); }
				let _ = write!(msg, "{}={}", k, val);
			}
			msg.push('}');
		}

		// get span
		let mut span_str = None;
		if let Some(scope) = ctx.event_scope(event) {
			let mut tmp = String::new();
			for (i, span) in scope.from_root().enumerate() {
				if i > 0 { tmp.push_str("::"); }
				tmp = format!("{}{}", tmp, span.name());
			}
			span_str = Some(tmp);
		}

		let convertedLevel  = TracingLevelToHtraceMapper(event.metadata().level());
		if(!self.isTracing(&convertedLevel))
		{
			return;
		}

		let file = event.metadata().file().unwrap_or("<unknown>");
		let line = event.metadata().line().unwrap_or(0);

		let mut backtrace = vec![];
		if(self.isBacktrace(&convertedLevel))
		{
			backtrace = HTracer::backtrace(file);
		}

		if let Some(span) = span_str
		{
			let mut context = Context::default();
			context.name_set(span);

			let _span = Span::new(context);
			HTracer::trace(&msg, convertedLevel, file, line, backtrace);
		}
		else
		{
			HTracer::trace(&msg, convertedLevel, file, line, backtrace);
		}
	}
}

pub fn TracingLevelToHtraceMapper(level: &TracingLevel) -> Level {
	match *level {
		TracingLevel::ERROR => Level::ERROR,
		TracingLevel::WARN => Level::WARNING,
		TracingLevel::INFO => Level::NOTICE,
		TracingLevel::DEBUG => Level::NORMAL,
		TracingLevel::TRACE => Level::DEBUG,
	}
}

pub fn HtraceToTracingLevelMapper(level: &Level) -> TracingLevel {
	match level {
		Level::DEBUG => TracingLevel::TRACE,
		Level::DEBUGERR => TracingLevel::TRACE,
		Level::NORMAL => TracingLevel::DEBUG,
		Level::NOTICE => TracingLevel::INFO,
		Level::NOTICEDERR => TracingLevel::INFO,
		Level::WARNING => TracingLevel::WARN,
		Level::ERROR => TracingLevel::ERROR,
		Level::FATAL => TracingLevel::ERROR,
	}
}