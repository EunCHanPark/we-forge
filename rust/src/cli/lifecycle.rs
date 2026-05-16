use super::*;

pub fn start()   -> Result<()> { service::manager().start() }
pub fn stop()    -> Result<()> { service::manager().stop() }
pub fn restart() -> Result<()> { service::manager().restart() }
