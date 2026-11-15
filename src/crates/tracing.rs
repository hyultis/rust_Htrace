use crate::components::level::Level;
use tracing::Level as TracingLevel;

use crate::crates::bridge::HtraceBridge;
use crate::htracer::HTracer;
use std::borrow::Cow;
use std::fmt;
use tracing::{
	Event, Subscriber,
	field::{Field, Visit},
	span::{Attributes, Id},
};
use tracing_subscriber::{
	layer::{Context as TContext, Layer},
	registry::{LookupSpan, SpanRef},
};

/// Path of a tracing span ("root::child::..."), computed once when the span is
/// created and stored in its extensions.
struct HtraceSpanPath(String);

/// Join the names of `span` and its ancestors visible to this layer, from the root.
fn spanPath_compute<S>(span: &SpanRef<'_, S>) -> String
where
	S: for<'a> LookupSpan<'a>,
{
	let mut path = String::new();
	for (i, span) in span.scope().from_root().enumerate()
	{
		if i > 0
		{
			path.push_str("::");
		}
		path.push_str(span.name());
	}
	path
}

struct Visitor
{
	message: Option<String>,
	fields: Vec<(String, String)>,
}
impl Visitor
{
	fn new() -> Self
	{
		Self {
			message: None,
			fields: Vec::new(),
		}
	}
}
impl Visit for Visitor
{
	fn record_str(&mut self, field: &Field, value: &str)
	{
		if field.name() == "message"
		{
			self.message = Some(value.to_owned());
		}
		else
		{
			self.fields
				.push((field.name().to_string(), value.to_owned()));
		}
	}
	fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug)
	{
		if field.name() == "message"
		{
			self.message = Some(format!("{value:?}"));
		}
		else
		{
			self.fields
				.push((field.name().to_string(), format!("{value:?}")));
		}
	}
}

impl<S> Layer<S> for HtraceBridge
where
	S: Subscriber + for<'a> LookupSpan<'a>,
{
	fn on_new_span(&self, _: &Attributes<'_>, id: &Id, ctx: TContext<'_, S>)
	{
		let Some(span) = ctx.span(id)
		else
		{
			return;
		};
		let parentPath = span.parent().and_then(|parent| {
			parent
				.extensions()
				.get::<HtraceSpanPath>()
				.map(|path| format!("{}::{}", path.0, span.name()))
		});
		let path = parentPath.unwrap_or_else(|| spanPath_compute(&span));
		// `replace` rather than `insert`: never panics if the layer is installed twice.
		span.extensions_mut().replace(HtraceSpanPath(path));
	}

	fn on_event(&self, event: &Event<'_>, ctx: TContext<'_, S>)
	{
		let convertedLevel = TracingLevelToHtraceMapper(event.metadata().level());
		if !self.isTracing(&convertedLevel) || !HTracer::enabled(convertedLevel)
		{
			return;
		}
		let mut v = Visitor::new();
		event.record(&mut v);

		// message + fields
		let mut msg = v.message.unwrap_or_else(|| "<no message>".to_string());
		if !v.fields.is_empty()
		{
			use std::fmt::Write;
			msg.push_str(" {");
			for (i, (k, val)) in v.fields.iter().enumerate()
			{
				if i > 0
				{
					msg.push_str(", ");
				}
				let _ = write!(msg, "{}={}", k, val);
			}
			msg.push('}');
		}

		// Span path of the event's parent (explicit or current), if any.
		let span = ctx.event_span(event);

		let file: Cow<'static, str> = Cow::Borrowed(event.metadata().file().unwrap_or("<unknown>"));
		let line = event.metadata().line().unwrap_or(0);

		let mut backtrace = None;
		if (self.isBacktrace(&convertedLevel))
		{
			backtrace = Some(HTracer::backtrace(file.clone()));
		}

		match span
		{
			// The resolved context is reused with only its name replaced: no span
			// is installed, so no context-exit notification follows the event.
			Some(span) => HTracer::trace_named(
				move || msg,
				|| match span.extensions().get::<HtraceSpanPath>()
				{
					Some(path) => path.0.clone(),
					None => spanPath_compute(&span),
				},
				convertedLevel,
				file,
				line,
				backtrace,
			),
			None => HTracer::trace_message(move || msg, convertedLevel, file, line, backtrace),
		}
	}
}

pub fn TracingLevelToHtraceMapper(level: &TracingLevel) -> Level
{
	match *level
	{
		TracingLevel::ERROR => Level::ERROR,
		TracingLevel::WARN => Level::WARNING,
		TracingLevel::INFO => Level::NOTICE,
		TracingLevel::DEBUG => Level::NORMAL,
		TracingLevel::TRACE => Level::DEBUG,
	}
}
