use crate::components::trace::OneTrace;
use regex::Regex;
use std::collections::HashMap;
use time::macros::format_description;

pub const FORMATTER_VARIABLE: [&str; 8] = [
	"time", "lvl", "file", "line", "msg", "thread", "context", "extra",
];

pub type FormaterCompilerSignature = fn(formater: &String) -> FormaterCompiled;
pub type FormaterParamBuilderSignature = fn(&OneTrace, &String) -> HashMap<String, String>;

pub struct FormaterCompiled
{
	pub inner: Vec<(String, Option<FormaterData>)>,
}

/// used to store and render the pre-compiled formater
impl FormaterCompiled
{
	/// render the pre-compiled formater
	/// parameters is a simple array created with FormaterParamBuilder
	pub fn render(&self, parameters: HashMap<String, String>) -> String
	{
		// Upper bound of the output (plus a trailing line break the outputs
		// append), so rendering never reallocates.
		let mut capacity = 1 + parameters.values().map(String::len).sum::<usize>();
		for (previous, data) in &self.inner
		{
			capacity += previous.len();
			if let Some(data) = data
			{
				capacity += data.prefix.len() + data.suffix.len();
			}
		}
		let mut rendered = String::with_capacity(capacity);
		for (previous, data) in &self.inner
		{
			rendered.push_str(previous);
			if let Some(data) = data
			{
				if let Some(content) = parameters
					.get(&data.data)
					.filter(|content| !content.is_empty())
				{
					rendered.push_str(&data.prefix);
					rendered.push_str(content);
					rendered.push_str(&data.suffix);
				}
			}
		}
		rendered
	}
}

/// used to store data name, and also it's prefix/suffix
#[derive(Debug)]
pub struct FormaterData
{
	pub prefix: String,
	pub data: String,
	pub suffix: String,
}

/// default formater for Htrace
/// compile the formater into a faster array inside a FormaterCompiled.render()
/// you can replace it with FormaterCompilerSignature (and put in inside config of module)
/// example of a formater string : `{time} {lvl} ({thread:>, }{file}:l{line} |{extra[test]}|) : {msg}`
/// each variable is present in FORMATTER_VARIABLE
/// extra is special, it can be used to get extra information from thread or context
pub fn FormaterCompile(formater: &String) -> FormaterCompiled
{
	let mut compiled = vec![];
	let mut lastchar = 0;
	let regexstr = format!(
		r"\{{({})(:([><])([^}}]+))?\}}",
		FORMATTER_VARIABLE
			.map(|var| {
				if (var == "extra")
				{
					return r"extra\[([^\]]+)\]";
				}
				return var;
			})
			.join("|")
	);
	let regex = Regex::new(&regexstr).unwrap();
	let _ = regex
		.replace_all(formater, |caps: &regex::Captures| {
			let mut data = caps.get(1).unwrap().as_str().to_string();
			let previous = &formater[lastchar..caps.get(0).unwrap().start()];
			lastchar = caps.get(0).unwrap().end();

			//when extra
			if (data.starts_with("extra"))
			{
				if let Some(extrakey) = caps.get(2)
				{
					data = format!("extra:{}", extrakey.as_str());
				}
			}

			// prefix/suffix
			let mut prefix = "".to_string();
			let mut suffix = "".to_string();
			if let Some(indicator) = caps.get(4)
			{
				if let Some(affix) = caps.get(5)
				{
					if (indicator.as_str() == "<")
					{
						prefix = affix.as_str().to_string();
					}
					else
					{
						suffix = affix.as_str().to_string();
					}
				}
			}

			compiled.push((
				previous.to_string(),
				Some(FormaterData {
					prefix: prefix,
					data,
					suffix: suffix,
				}),
			));
			"".to_string()
		})
		.into_owned();

	compiled.push((formater[lastchar..].to_string(), None));

	return FormaterCompiled { inner: compiled };
}

/// Append `value` to `output`, replacing each real line break with
/// `replacement`, in a single pass. `\r\n` and `\n\r` pairs count as one
/// break; literal backslash sequences (such as the two characters `\` and `n`)
/// are kept verbatim.
fn pushReplacingLineBreaks(output: &mut String, value: &str, replacement: &[&str])
{
	let mut rest = value;
	while let Some(position) = rest.find(['\n', '\r'])
	{
		output.push_str(&rest[..position]);
		replacement.iter().for_each(|part| output.push_str(part));
		let complement = match rest.as_bytes()[position]
		{
			b'\n' => b'\r',
			_ => b'\n',
		};
		let mut next = position + 1;
		if rest.as_bytes().get(next) == Some(&complement)
		{
			next += 1;
		}
		rest = &rest[next..];
	}
	output.push_str(rest);
}

/// Keep metadata fields on one line: each line break becomes a single space.
fn flattenLineBreaks(value: &str) -> String
{
	let mut flattened = String::with_capacity(value.len());
	pushReplacingLineBreaks(&mut flattened, value, &[" "]);
	flattened
}

/// format a string in a simple way errorless
/// you can replace it with FormaterParamBuilderSignature (and put in inside config of module)
/// Real line breaks in `{msg}` are followed by `lineReturn`; line breaks in
/// `{thread}`, `{context}`, `{file}` and `{extra[*]}` are replaced by a space.
pub fn FormaterParamBuilder(trace: &OneTrace, lineReturn: &String) -> HashMap<String, String>
{
	// Message and backtraces are built into a single buffer.
	let mut msg = String::with_capacity(match trace.backtraces.is_empty()
	{
		true => trace.message.len(),
		false => trace.message.len() + 9 + trace.backtraces.len() * (lineReturn.len() + 65),
	});
	pushReplacingLineBreaks(&mut msg, &trace.message, &["\n", lineReturn]);

	if !trace.backtraces.is_empty()
	{
		use std::fmt::Write;
		msg.push_str(", with : ");
		for one in &trace.backtraces
		{
			let _ = write!(&mut msg, "\n{lineReturn}{one}");
		}
	}

	let extras = trace.context.extra_getAll();
	let mut parameters = HashMap::with_capacity(8 + extras.len());
	let formatTime = format_description!("[hour repr:24]:[minute]:[second].[subsecond digits:6]");
	parameters.insert(
		"time".to_string(),
		trace
			.date
			.format(formatTime)
			.unwrap_or("00:00:00.000000".to_string()),
	);
	parameters.insert("lvl".to_string(), trace.level.convert4LengthString());
	parameters.insert(
		"thread".to_string(),
		flattenLineBreaks(trace.context.threadName_get().as_deref().unwrap_or("")),
	);
	parameters.insert(
		"context".to_string(),
		flattenLineBreaks(trace.context.name_get().as_deref().unwrap_or("")),
	);
	parameters.insert("file".to_string(), flattenLineBreaks(&trace.filename));
	parameters.insert("line".to_string(), trace.fileline.to_string());

	extras.iter().for_each(|(key, data)| {
		parameters.insert(format!("extra:{}", key), flattenLineBreaks(data));
	});

	parameters.insert("msg".to_string(), msg);
	return parameters;
}
#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn conditional_affixes_and_unknown_tokens_are_preserved()
	{
		let compiled = FormaterCompile(&"a{context:<[}{context:>]}b{msg}c{unknown}".into());
		assert_eq!(compiled.render(HashMap::new()), "abc{unknown}");
		assert_eq!(
			compiled.render(HashMap::from([
				("context".into(), "x".into()),
				("msg".into(), "hello".into())
			])),
			"a[xx]bhelloc{unknown}"
		);
	}

	#[test]
	fn optional_variable_renders_affixes_only_when_present()
	{
		let compiled = FormaterCompile(&"[{extra[test]:<(}{extra[test]:>) }{msg}]".into());
		let mut context = crate::components::context::Context::default();
		let absent = FormaterParamBuilder(
			&traceWith("hello", context.clone(), "file.rs"),
			&" | ".to_string(),
		);
		assert_eq!(compiled.render(absent), "[hello]");
		context.extra_set("test", "");
		let empty = FormaterParamBuilder(
			&traceWith("hello", context.clone(), "file.rs"),
			&" | ".to_string(),
		);
		assert_eq!(compiled.render(empty), "[hello]");
		context.extra_set("test", "x");
		let present =
			FormaterParamBuilder(&traceWith("hello", context, "file.rs"), &" | ".to_string());
		assert_eq!(compiled.render(present), "[(xx) hello]");
	}

	#[test]
	fn backtraces_follow_the_message_unchanged()
	{
		let mut trace = traceWith("a\nb", Default::default(), "file.rs");
		trace.backtraces = vec![
			crate::components::hbacktrace::Hbacktrace {
				funcName: "app::run".into(),
				fileName: Some("src/app.rs".into()),
				line: Some(3),
			},
			crate::components::hbacktrace::Hbacktrace {
				funcName: "main".into(),
				fileName: None,
				line: None,
			},
		];
		let parameters = FormaterParamBuilder(&trace, &" | ".to_string());
		assert_eq!(
			parameters.get("msg").map(String::as_str),
			Some("a\n | b, with : \n | src/app.rs(3): app::run\n | - main")
		);
	}

	fn traceWith(
		message: &str,
		context: crate::components::context::Context,
		filename: &str,
	) -> OneTrace
	{
		OneTrace {
			message: message.to_string(),
			date: time::macros::datetime!(2026-01-01 00:00 UTC),
			level: crate::components::level::Level::NORMAL,
			context: std::sync::Arc::new(context),
			filename: filename.to_string().into(),
			fileline: 1,
			backtraces: Vec::new(),
		}
	}

	fn renderedMessage(message: &str) -> String
	{
		let parameters = FormaterParamBuilder(
			&traceWith(message, Default::default(), "file.rs"),
			&" | ".to_string(),
		);
		parameters.get("msg").cloned().unwrap_or_default()
	}

	#[test]
	fn literal_backslash_sequences_are_kept_verbatim()
	{
		for message in [r"C:\new\rapport", r"a\nb\rc\n\rd"]
		{
			assert_eq!(renderedMessage(message), message);
		}
	}

	#[test]
	fn real_line_breaks_produce_one_continuation_each()
	{
		for message in ["a\nb", "a\rb", "a\r\nb", "a\n\rb"]
		{
			assert_eq!(renderedMessage(message), "a\n | b", "{message:?}");
		}
		assert_eq!(renderedMessage("a\n\nb"), "a\n | \n | b");
		assert_eq!(renderedMessage("a\r\n\r\nb"), "a\n | \n | b");
		assert_eq!(renderedMessage("a\n\r\nb"), "a\n | \n | b");
	}

	#[test]
	fn metadata_line_breaks_are_flattened_to_spaces()
	{
		let mut context = crate::components::context::Context::default();
		context.threadName_set("thread\none");
		context.name_set("context\r\nname");
		context.extra_set("key", "first\n\rsecond\rthird");
		let parameters = FormaterParamBuilder(
			&traceWith("msg\nnext", context, "dir\nfile.rs"),
			&" | ".to_string(),
		);
		let compiled = FormaterCompile(&"{thread}|{context}|{file}|{extra[key]}|{msg}".into());
		assert_eq!(
			compiled.render(parameters),
			"thread one|context name|dir file.rs|first second third|msg\n | next"
		);
	}
}
