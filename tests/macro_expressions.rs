// Every macro arm must be usable in expression position without the
// future-incompatible trailing semicolon lint.
#![deny(semicolon_in_expressions_from_macros)]
#![deny(semicolon_in_expressions_from_non_local_macros)]
#![allow(non_snake_case)]

use htrace::components::level::Level;
use htrace::{HTrace, HTraceError};

fn dispatch(kind: u8, value: u32, result: &Result<(), String>)
{
	match kind
	{
		0 => HTrace!((Level::NOTICE) "explicit level {}", value),
		1 => HTrace!((Level::NOTICE) value),
		2 => HTrace!("default level {}", value),
		3 => HTrace!(value),
		4 => HTraceError!((Level::WARNING) "explicit level {:?}", result),
		5 => HTraceError!((Level::WARNING) result),
		6 => HTraceError!("default level {:?}", result),
		_ => HTraceError!(result),
	}
}

#[test]
fn macros_are_expressions()
{
	let result: Result<(), String> = Err("expression".into());
	for kind in 0..=7
	{
		dispatch(kind, 42, &result);
	}
	let formatted = |value: u32| HTrace!("closure {}", value);
	let raw = |value: u32| HTrace!(value);
	let error = |result: &Result<(), String>| HTraceError!("closure {:?}", result);
	let rawError = |result: &Result<(), String>| HTraceError!(result);
	formatted(1);
	raw(2);
	error(&result);
	rawError(&result);
	assert!(result.is_err());
}
