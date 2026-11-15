use crate::components::level::Level;
#[cfg(feature = "tracing_consumer")]
use tracing::subscriber::Interest;
#[cfg(feature = "tracing_consumer")]
use tracing::{Event, Metadata, Subscriber};
#[cfg(feature = "tracing_consumer")]
use tracing_subscriber::layer::{Context, Filter, Layer};
#[cfg(feature = "tracing_consumer")]
use tracing_subscriber::registry::LookupSpan;

/// Span-based filter for `tracing` events. Only events emitted inside at least
/// one span are filtered: events outside any span always pass. Ancestors are
/// the event's parent span (explicit or current) and all its parents; names
/// are compared exactly. Empty lists have no special meaning.
#[cfg(feature = "tracing_consumer")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum HtraceBridgeSpanFilterList
{
	/// Every event passes (default).
	#[default]
	AllowAll,
	/// Every event inside a span is rejected.
	DenyAll,
	/// An event inside a span passes only if at least one ancestor span has a
	/// listed name (`Allow(vec![])` rejects every event inside a span).
	Allow(Vec<String>),
	/// An event inside a span is rejected if at least one ancestor span has a
	/// listed name (`Deny(vec![])` lets everything pass).
	Deny(Vec<String>),
}

/// Per-layer filter applying `HtraceBridgeSpanFilterList` to events. Spans
/// themselves stay enabled so that the full ancestry remains visible.
#[cfg(feature = "tracing_consumer")]
struct SpanFilter
{
	list: HtraceBridgeSpanFilterList,
	/// Minimum level the bridge accepts (`tracing_min_level`).
	minLevel: Level,
}

#[cfg(feature = "tracing_consumer")]
impl SpanFilter
{
	/// Spans always pass; events pass from `tracing_min_level`.
	fn levelEnabled(&self, metadata: &Metadata<'_>) -> bool
	{
		!metadata.is_event()
			|| crate::crates::tracing::TracingLevelToHtraceMapper(metadata.level()).tou8()
				>= self.minLevel.tou8()
	}
}

#[cfg(feature = "tracing_consumer")]
impl<S> Filter<S> for SpanFilter
where
	S: Subscriber + for<'a> LookupSpan<'a>,
{
	fn enabled(&self, metadata: &Metadata<'_>, _: &Context<'_, S>) -> bool
	{
		self.levelEnabled(metadata)
	}

	/// Event callsites below `tracing_min_level` are disabled for this layer
	/// only (other layers are unaffected); spans are never disabled, so span
	/// paths and the span list keep seeing every ancestor. No level hint is
	/// reported. Events that may be rejected by the span list stay `sometimes`:
	/// tracing-subscriber expects `enabled` to run before an `event_enabled`
	/// that can reject (its debug assertions fail otherwise).
	fn callsite_enabled(&self, metadata: &'static Metadata<'static>) -> Interest
	{
		if !self.levelEnabled(metadata)
		{
			return Interest::never();
		}
		match metadata.is_event() && self.list != HtraceBridgeSpanFilterList::AllowAll
		{
			true => Interest::sometimes(),
			false => Interest::always(),
		}
	}

	fn event_enabled(&self, event: &Event<'_>, cx: &Context<'_, S>) -> bool
	{
		let (names, allow) = match &self.list
		{
			HtraceBridgeSpanFilterList::AllowAll => return true,
			HtraceBridgeSpanFilterList::DenyAll => return cx.event_span(event).is_none(),
			HtraceBridgeSpanFilterList::Allow(names) => (names, true),
			HtraceBridgeSpanFilterList::Deny(names) => (names, false),
		};
		let Some(mut ancestors) = cx.event_scope(event)
		else
		{
			return true;
		};
		let listed = ancestors.any(|span| {
			let spanName = span.name();
			names.iter().any(|name| name.as_str() == spanName)
		});
		listed == allow
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HtraceBridge
{
	/// minimum level for backtrace (default error, like the macros; use None to disable, use Some(Level::min()) to enable all)
	pub min_level_backtrace: Option<Level>,

	/// Htrace.level::DEBUG is the minimum level (that must be set here), correspond to: Log.level::Trace is the maximum level
	#[cfg(feature = "log_consumer")]
	pub log_min_level: Level,
	/// Htrace.level::DEBUG is the minimum level (that must be set here), correspond to: Tracing.level::Trace is the maximum level
	///
	/// Through `filtered()` (as installed by `globalContext_set`), event
	/// callsites below this level are disabled for the bridge layer; spans of any
	/// level stay enabled, so span paths and `tracing_filter_span` are unaffected.
	#[cfg(feature = "tracing_consumer")]
	pub tracing_min_level: Level,

	/// span filter list for tracing
	#[cfg(feature = "tracing_consumer")]
	pub tracing_filter_span: HtraceBridgeSpanFilterList,
}

impl HtraceBridge
{
	/// return if a level is a backtrace level (equal or above the "min_level_backtrace")
	pub fn isBacktrace(&self, level: &Level) -> bool
	{
		if let Some(backtracelevel) = self.min_level_backtrace
		{
			return level.tou8() >= backtracelevel.tou8();
		}
		return false;
	}

	#[cfg(feature = "log_consumer")]
	/// return if a level is a log level (equal or above the "log_min_level")
	pub fn isLog(&self, level: &Level) -> bool
	{
		return level.tou8() >= self.log_min_level.tou8();
	}

	#[cfg(feature = "tracing_consumer")]
	/// return if a level is a tracing level (equal or above the "min_level_tracing")
	pub fn isTracing(&self, level: &Level) -> bool
	{
		return level.tou8() >= self.tracing_min_level.tou8();
	}

	#[cfg(feature = "tracing_consumer")]
	/// return the bridge layer filtered by `tracing_filter_span`
	pub fn filtered<S>(self) -> impl Layer<S>
	where
		S: for<'a> LookupSpan<'a> + Subscriber,
	{
		let filter = SpanFilter {
			list: self.tracing_filter_span.clone(),
			minLevel: self.tracing_min_level,
		};
		self.with_filter(filter)
	}
}

/// Backtraces start at ERROR, like the macros.
impl Default for HtraceBridge
{
	fn default() -> Self
	{
		HtraceBridge {
			min_level_backtrace: Some(Level::ERROR),
			#[cfg(feature = "log_consumer")]
			log_min_level: Level::DEBUG,
			#[cfg(feature = "tracing_consumer")]
			tracing_min_level: Level::DEBUG,
			#[cfg(feature = "tracing_consumer")]
			tracing_filter_span: Default::default(),
		}
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn default_backtrace_level_matches_the_macros()
	{
		let bridge = HtraceBridge::default();
		assert_eq!(bridge.min_level_backtrace, Some(Level::ERROR));
		assert!(!bridge.isBacktrace(&Level::WARNING));
		assert!(bridge.isBacktrace(&Level::ERROR));
		assert!(bridge.isBacktrace(&Level::FATAL));
	}
}
