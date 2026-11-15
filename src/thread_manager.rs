/// Producer name used for every trace unless a span sets `Context::threadName_set`.
/// The native OS thread name is never used.
pub(crate) const MAIN_THREAD: &str = "MAIN";
