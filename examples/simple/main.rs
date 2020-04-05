use redox_log::RedoxLogger;
use log::{debug, error, info, trace, warn};

fn main() {
    RedoxLogger::new_from_file(std::fs::File::create("file.log").unwrap()).unwrap().with_stdout_mirror().with_flush_after_write(true).enable().unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    info!("Example started");
    debug!("example started with log file: {}", "file.log");
    trace!("useless comment");
    warn!("useless comment is useless");
    error!("deadlock");
    loop {}
}
