use std::collections::HashMap;
use time::macros::format_description;
use crate::components::trace::OneTrace;
use regex::Regex;

pub const FORMATTER_VARIABLE: [&str; 8] = ["time","lvl","file","line","msg","thread","context","extra"];

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
		return self.inner.iter().map(|(previous,data)|{
			let Some(data) = data else {
				return previous.clone();
			};
			format!("{}{}{}{}", previous, data.prefix, parameters.get(&data.data).unwrap_or(&String::new()), data.suffix)
		}).collect::<Vec<String>>().join("");
	}
}

/// used to store data name, and also it's prefix/suffix
#[derive(Debug)]
pub struct FormaterData {
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
	let regexstr = format!(r"\{{({})(:([><])([^}}]+))?\}}", FORMATTER_VARIABLE.map(|var|{
		if(var=="extra")
		{
			return r"extra\[([^\]]+)\]";
		}
		return var;
	}).join("|"));
	let regex = Regex::new(&regexstr).unwrap();
	let _ = regex.replace_all(formater, |caps: &regex::Captures| {
		let mut data = caps.get(1).unwrap().as_str().to_string();
		let previous = &formater[lastchar..caps.get(0).unwrap().start()];
		lastchar = caps.get(0).unwrap().end();

		//when extra
		if(data.starts_with("extra"))
		{
			if let Some(extrakey) = caps.get(2)
			{
				data=format!("extra:{}",extrakey.as_str());
			}
		}

		// prefix/suffix
		let mut prefix = "".to_string();
		let mut suffix = "".to_string();
		if let Some(indicator) = caps.get(4)
		{
			if let Some(affix) = caps.get(5)
			{
				if(indicator.as_str()=="<")
				{
					prefix = affix.as_str().to_string();
				}
				else
				{
					suffix = affix.as_str().to_string();
				}
			}
		}

		compiled.push((previous.to_string(), Some(FormaterData{
			prefix: prefix,
			data,
			suffix: suffix,
		})));
		"".to_string()
	}).into_owned();

	compiled.push((formater[lastchar..].to_string(), None));

	println!("compiled {:?}",compiled);

	return FormaterCompiled{ inner: compiled };
}


/// format a string in a simple way errorless
/// you can replace it with FormaterParamBuilderSignature (and put in inside config of module)
pub fn FormaterParamBuilder(trace: &OneTrace, lineReturn: &String) -> HashMap<String, String>
{
	let mut msg = trace.message.clone();
	if(msg.contains("\n") || msg.contains("\r") || msg.contains("\\n") || msg.contains("\\r"))
	{
		let linereturn = format!("\n{}",lineReturn);
		msg = msg.replace("\n\r","\n");
		msg = msg.replace("\\n\\r","\n");
		msg = msg.replace("\r","\n");
		msg = msg.replace("\\r","\n");
		msg = msg.replace("\\n","\n");
		msg = msg.replace("\n",linereturn.as_str());
	}

	if(trace.backtraces.len()>0)
	{
		let mut drawBacktraces= "".to_string();
		trace.backtraces.iter().for_each(|one|{
			drawBacktraces = format!("{}\n{}{}",drawBacktraces,lineReturn,one.to_string());
		});

		msg = format!("{}, with : {}",msg,drawBacktraces)
	}

	let mut parameters = HashMap::new();
	let formatTime = format_description!("[hour repr:24]:[minute]:[second].[subsecond digits:6]");
	parameters.insert("time".to_string(), trace.date.format(formatTime).unwrap_or("00:00:00.000000".to_string()));
	parameters.insert("lvl".to_string(), trace.level.convert4LengthString());
	parameters.insert("thread".to_string(), trace.context.threadName_get().clone().unwrap_or("".to_string()));
	parameters.insert("context".to_string(), trace.context.name_get().clone().unwrap_or("".to_string()));
	parameters.insert("file".to_string(), trace.filename.clone());
	parameters.insert("line".to_string(), trace.fileline.to_string());

	trace.context.extra_getAll().iter().for_each(|(key,data)|{
		parameters.insert(format!("extra:{}",key), data.to_string());
	});

	parameters.insert("msg".to_string(), msg);
	return parameters;
}