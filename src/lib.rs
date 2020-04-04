use std::io::BufWriter;
use std::io::prelude::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{io, fs};

use log::{Metadata, Record};

pub struct RedoxLogger {
    file: Mutex<BufWriter<Box<dyn Write + Send + 'static>>>,
    stdout: Option<io::Stdout>,
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
    pub fn new_from_file<W: Write + Send + 'static>(logfile: Box<W>) -> Result<Self, io::Error> {
        let file = Mutex::new(BufWriter::new(logfile as Box<_>));

        // TODO: Log rotation: older log files from previous executions of the program should be
        // compressed, and stored elsewhere.
        // TODO: Also, a dedicated daemon should perhaps in the future replace log files, or simply
        // complement them.

        Ok(Self {
            file,
            stdout: None,
        })
    }
    pub fn with_stdout_mirror(self) -> Self {
        Self {
            stdout: Some(io::stdout()),
            .. self
        }
    }
    pub fn enable(self) -> Result<(), log::SetLoggerError> {
        log::set_logger(Box::leak(Box::new(self)))
    }
    fn write_record<W: Write>(record: &Record, writer: &mut W) -> io::Result<()> {
        // TODO: Log offloading to another thread or thread pool, maybe?

        let now_local = chrono::Local::now();
        let time = now_local.format("%Y-%m-%dT%H-%M-%S.%.3f+%:z");

        let module_path = record.module_path();
        let target = record.target();
        let module_path_str = module_path.unwrap_or("");
        let coloncolon = if module_path.is_some() { "::" } else { "" };

        let level = record.level();

        writeln!(writer, "{time:}[{mpath:}{cc:}{target:}] {level:} {msg:}", time=time, mpath=module_path_str, cc=coloncolon, target=target, level=level, msg=record.args())
    }
}

impl log::Log for RedoxLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // TODO: Configure the level filter using environment variables. Alternatively unless the throughput becomes too much, the
        // redox version of journald (to be written) would handle that.
        true
    }
    fn log(&self, record: &Record) {
        let _ = Self::write_record(record, &mut *self.file.lock().unwrap());
        if let Some(ref stdout) = self.stdout {
            let _ = Self::write_record(record, &mut stdout.lock());
        }
    }
    fn flush(&self) {
        let _ = self.file.lock().unwrap().flush();
    }
}
