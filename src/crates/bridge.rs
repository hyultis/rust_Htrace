use tracing::Metadata;
use crate::components::level::Level;

#[cfg(feature = "tracing_consumer")]
use tracing_subscriber::{Layer};
#[cfg(feature = "tracing_consumer")]
use tracing_subscriber::filter::dynamic_filter_fn;
#[cfg(feature = "tracing_consumer")]
use tracing_subscriber::layer::Context;

#[cfg(feature = "tracing_consumer")]
#[derive(Debug,Clone)]
pub enum HtraceBridgeSpanFilterList {
	/// Allow only some specific spans (empty list = deny all)
	Allow(Vec<String>),
	/// Deny some specific spans (empty list = allow all)
	Deny(Vec<String>),
}

#[cfg(feature = "tracing_consumer")]
impl Default for HtraceBridgeSpanFilterList {
	fn default() -> Self {
		HtraceBridgeSpanFilterList::Deny(vec![])
	}
}

#[derive(Clone)]
pub struct HtraceBridge {
	/// minimum level for backtrace (default warn, use None to disable, use Some(Level::min()) to enable all)
	pub min_level_backtrace: Option<Level>,

	/// Htrace.level::DEBUG is the minimum level (that must be set here), correspond to: Log.level::Trace is the maximum level
	#[cfg(feature = "log_consumer")]
	pub log_min_level: Level,
	/// Htrace.level::DEBUG is the minimum level (that must be set here), correspond to: Tracing.level::Trace is the maximum level
	#[cfg(feature = "tracing_consumer")]
	pub tracing_min_level: Level,

	/// span filter list for tracing
	#[cfg(feature = "tracing_consumer")]
	pub tracing_filter_span: HtraceBridgeSpanFilterList,
}

impl HtraceBridge {

	/// return if a level is a backtrace level (equal or above the "min_level_backtrace")
	pub fn isBacktrace(&self, level: &Level) -> bool {
		if let Some(backtracelevel) = self.min_level_backtrace
		{
			return level.tou8() >= backtracelevel.tou8();
		}
		return false;
	}


	#[cfg(feature = "log_consumer")]
	/// return if a level is a log level (equal or above the "log_min_level")
	pub fn isLog(&self, level: &Level) -> bool {
		return level.tou8() >= self.log_min_level.tou8();
	}

	#[cfg(feature = "tracing_consumer")]
	/// return if a level is a tracing level (equal or above the "min_level_tracing")
	pub fn isTracing(&self, level: &Level) -> bool {
		return level.tou8() >= self.tracing_min_level.tou8();
	}

	#[cfg(feature = "tracing_consumer")]
	/// return a filtered layer for tracing
    pub fn filtered<S>(self) -> impl Layer<S>
    where
        S: for<'a> tracing_subscriber::registry::LookupSpan<'a> + tracing::Subscriber,
    {
        let filter_list = self.tracing_filter_span.clone();
        let only_requests_or_user = dynamic_filter_fn(move |_meta: &Metadata<'_>, ctx: &Context<'_, S>| {
	        if let Some(span) = ctx.lookup_current()
            {
                match &filter_list {
                    HtraceBridgeSpanFilterList::Allow(allowed) => {
	                    //println!("span {} : ALLOW ? {}",&span.name(),allowed.contains(&span.name().to_string()));
                        if allowed.contains(&span.name().to_string()) {
                            return true;
                        }
                    }
                    HtraceBridgeSpanFilterList::Deny(denied) => {
	                    //println!("span {} : DENY ? {}",&span.name(),denied.contains(&span.name().to_string()));
                        if !denied.contains(&span.name().to_string()) {
                            return true;
                        }
                    }
                }
	            return false;
            };

	        return true;
        });

        self.with_filter(only_requests_or_user)
    }
}

impl Default for HtraceBridge {
	fn default() -> Self {
		HtraceBridge {
			min_level_backtrace: Some(Level::WARNING),
			#[cfg(feature = "log_consumer")]
			log_min_level: Level::DEBUG,
			#[cfg(feature = "tracing_consumer")]
			tracing_min_level: Level::DEBUG,
			#[cfg(feature = "tracing_consumer")]
			tracing_filter_span: Default::default(),
		}
	}
}