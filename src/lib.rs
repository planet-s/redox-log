use std::io::BufWriter;
use std::io::prelude::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{io, fs};

use log::{Metadata, Record};

trait WriteAndSeek: Write + Seek + Send + 'static {}
impl<T> WriteAndSeek for T where T: Write + Seek + Send + 'static {}

pub struct RedoxLogger {
    file: Mutex<BufWriter<Box<dyn WriteAndSeek>>>,
}

impl RedoxLogger {
    pub fn new<A: AsRef<Path>, B: AsRef<Path>, C: AsRef<Path>>(category: A, subcategory: B, logfile: C) -> Result<Self, io::Error> {
        let mut path = PathBuf::from("logging:");
        path.push(category);
        path.push(subcategory);
        path.push(logfile);
        path.set_extension("log");

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        Ok(Self::new_from_file(Box::new(File::create(path)?))?)
    }
    pub fn new_from_file<W: Write + Seek + Send + 'static>(logfile: Box<W>) -> Result<Self, io::Error> {
        let file = Mutex::new(BufWriter::new(logfile as Box<_>));

        // TODO: Log rotation: older log files from previous executions of the program should be
        // compressed, and stored elsewhere.
        // TODO: Also, a dedicated daemon should perhaps in the future replace log files, or simply
        // complement them.

        Ok(Self {
            file,
        })
    }
    pub fn enable(self) -> Result<(), log::SetLoggerError> {
        log::set_logger(Box::leak(Box::new(self)))
    }
}

impl log::Log for RedoxLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // TODO: Configure the level filter using environment variables. Alternatively unless the throughput becomes too much, the
        // redox version of journald (to be written) would handle that.
        true
    }
    fn log(&self, record: &Record) {
        // TODO: Log offloading to another thread or thread pool, maybe?
        // TODO: Stdout/stderr mirroring, with colored text.

        let now_local = chrono::Local::now();
        let time = now_local.format("%Y-%m-%dT%H-%M-%S.%.3f+%:z");

        let module_path = record.module_path();
        let target = record.target();
        let module_path_str = module_path.unwrap_or("");
        let coloncolon = if module_path.is_some() { "::" } else { "" };

        let level = record.level();

        writeln!(self.file.lock().unwrap(), "{time:}[{mpath:}{cc:}{target:}] {level:} {msg:}", time=time, mpath=module_path_str, cc=coloncolon, target=target, level=level, msg=record.args()).unwrap();
    }
    fn flush(&self) {
        let _ = self.file.lock().unwrap().flush();
    }
}
