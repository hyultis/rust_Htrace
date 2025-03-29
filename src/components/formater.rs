use std::collections::HashMap;
use time::macros::format_description;
use crate::components::trace::OneTrace;

pub type FormaterBuilderSignature = fn(&OneTrace, &String) -> HashMap<String, String>;
pub type FormaterSignature = fn(&String, HashMap<String, String>) -> String;

/// format a string in a simple way errorless
pub fn HtraceDefaultFormaterBuilder(trace: &OneTrace, lineReturn: &String) -> HashMap<String, String>
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
	parameters.insert("thread+,".to_string(), trace.context.threadName_get().clone().map(|e| format!("{}, ", e)).unwrap_or("".to_string()));
	parameters.insert("file".to_string(), trace.filename.clone());
	parameters.insert("line".to_string(), trace.fileline.to_string());
	parameters.insert("msg".to_string(), msg);
	return parameters;
}

pub fn HtraceDefaultFormater(formater: &String, parameters: HashMap<String, String>) -> String
{
	let mut formater = formater.clone();
	let key = ["time","lvl","file","line","msg","thread","thread+,"];
	key.iter().for_each(|key| {
		if let Some(value) = parameters.get(*key)
		{
			formater = formater.replace(format!("{{{}}}", key).as_str(), value);
		}
	});
	return formater;
}