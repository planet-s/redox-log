use std::io::BufWriter;
use std::io::prelude::*;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::Mutex;
use std::{io, fs};

use log::{Metadata, Record};

pub struct RedoxLogger {
    file: Mutex<BufWriter<File>>,
}

impl RedoxLogger {
    pub fn new<A: AsRef<Path>, B: AsRef<Path>>(dir: &A, name: &B) -> Result<Self, io::Error> {
        if fs::metadata(dir.as_ref()).err().map(|err| err.kind() == io::ErrorKind::NotFound).unwrap_or(false) {
            fs::create_dir_all(dir)?;
        }

        let path = dir.as_ref().join(name.as_ref());
        let file = Mutex::new(BufWriter::new(OpenOptions::new().create_new(true).read(false).write(true).append(true).open(path)?));

        // TODO: Log rotation: older log files from previous executions of the program should be
        // compressed, and stored elsewhere.
        // TODO: Also, a dedicated daemon should perhaps in the future replace log files, or simply
        // complement them.

        Ok(Self {
            file,
        })
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

        let _ = writeln!(self.file.lock().unwrap(), "{time:}[{mpath:}{cc:}{target:}] {level:} {msg:}", time=time, mpath=module_path_str, cc=coloncolon, target=target, level=level, msg=record.args());
    }
    fn flush(&self) {
        let _ = self.file.lock().unwrap().flush();
    }
}
