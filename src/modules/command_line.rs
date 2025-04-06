use crate::modules::module_abstract::ModuleAbstract;
use crate::components::trace::OneTrace;
use anyhow::Result;
use owo_colors::{OwoColorize, Style};
use crate::components::context::Context;
use crate::components::formater::FormaterCompiled;
use crate::modules::command_line_config::CommandLineConfig;

pub struct CommandLine
{
	_name: String,
	_configs: CommandLineConfig,
	_formaterCompiled: FormaterCompiled,
}

impl CommandLine
{
	pub fn new(config: CommandLineConfig) -> CommandLine {
		let binding = &config.formaterCompiler;
		let fmtComp = binding(&config.lineFormat);
		return CommandLine{
			_name: String::new(),
			_configs: config,
			_formaterCompiled: fmtComp,
		};
	}
	
	pub fn draw(&self,trace: &OneTrace)
	{
		let color = self._configs.colors.get(&trace.level).unwrap_or(&Style::new()).clone();

		let binding = &self._configs.formaterParamBuilder;
		let mut parameters = binding(trace, &self._configs.lineReturn);
		parameters.get_mut("lvl").iter_mut().for_each(|x| **x = x.style(color).to_string());
		parameters.get_mut("msg").iter_mut().for_each(|x| **x = x.style(color).to_string());

		println!("{}",self._formaterCompiled.render(parameters));
	}
}

impl ModuleAbstract for CommandLine
{
	fn name_set(&mut self, moduleName: String) -> Result<()> {
		self._name = moduleName;
		return Ok(());
	}
	
	fn name_get(&self) -> Result<String> {
		return Ok(self._name.clone());
	}
	
	fn event_onDebug(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onDebugErr(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onNormal(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onNotice(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onNoticeErr(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onWarning(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onError(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}
	
	fn event_onFatal(&self, trace: &OneTrace)
	{
		self.draw(trace);
	}

	fn event_onContextExit(&self, _: &Context) {}
	
	fn event_onGlobalExit(&self) {
	
	}
	
}
