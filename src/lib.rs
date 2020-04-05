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
    flush: bool,
}

impl RedoxLogger {
    #[cfg(any(target_os = "redox", rustdoc))]
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
    pub fn new_from_file<W: Write + Send + 'static>(logfile: W) -> Result<Self, io::Error> {
        let file = Mutex::new(BufWriter::new(Box::new(logfile) as Box<_>));

        // TODO: Log rotation: older log files from previous executions of the program should be
        // compressed, and stored elsewhere.
        // TODO: Also, a dedicated daemon should perhaps in the future replace log files, or simply
        // complement them.

        Ok(Self {
            file,
            stdout: None,
            flush: true,
        })
    }
    pub fn with_stdout_mirror(self) -> Self {
        Self {
            stdout: Some(io::stdout()),
            .. self
        }
    }
    pub fn with_flush_after_write(self, flush: bool) -> Self {
        Self {
            flush,
            .. self
        }
    }
    pub fn enable(self) -> Result<&'static Self, log::SetLoggerError> {
        let leak = Box::leak(Box::new(self));
        log::set_logger(leak)?;
        Ok(leak)
    }
    fn write_record<W: Write>(colored: bool, record: &Record, writer: &mut W) -> io::Result<()> {
        use std::fmt;
        use termion::{color, style};
        use log::Level;


        // TODO: Log offloading to another thread or thread pool, maybe?

        let now_local = chrono::Local::now();

        // TODO: Use colors in timezone, when colors are enabled, to e.g. gray out the timezone and
        // make the actual date more readable.
        let time = now_local.format("%Y-%m-%dT%H-%M-%S.%.3f+%:z");
        let target = record.target();
        let level = record.level();
        let message = record.args();

        let trace_col = color::Fg(color::LightBlack);
        let debug_col = color::Fg(color::White);
        let info_col = color::Fg(color::LightBlue);
        let warn_col = color::Fg(color::LightYellow);
        let err_col = color::Fg(color::LightRed);

        let level_color: &dyn fmt::Display = match level {
            Level::Trace => &trace_col,
            Level::Debug => &debug_col,
            Level::Info => &info_col,
            Level::Warn => &warn_col,
            Level::Error => &err_col,
        };

        let dim_white = color::Fg(color::White);
        let bright_white = color::Fg(color::LightWhite);
        let regular_style = "";
        let bold_style = style::Bold;

        let [message_color, message_style]: [&dyn fmt::Display; 2] = match level {
            Level::Trace | Level::Debug => [&dim_white, &regular_style],
            Level::Info | Level::Warn | Level::Error => [&bright_white, &bold_style],
        };
        let target_color = color::Fg(color::White);

        let time_color = color::Fg(color::LightBlack);

        let reset = color::Fg(color::Reset);

        writeln!(
            writer,
            "{time:} [{target:} {level:}] {msg:}",

            time=format_args!("{m:}{col:}{msg:}{rs:}{r:}", m=style::Italic, col=time_color, msg=time, r=reset, rs=style::Reset),
            level=format_args!("{m:}{col:}{msg:}{rs:}{r:}", m=style::Bold, col=level_color, msg=level, r=reset, rs=style::Reset),
            target=format_args!("{col:}{msg:}{r:}", col=target_color, msg=target, r=reset),
            msg=format_args!("{m:}{col:}{msg:}{rs:}{r:}", m=message_style, col=message_color, msg=message, r=reset, rs=style::Reset),
        )
    }
}

impl log::Log for RedoxLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // TODO: Configure the level filter using environment variables. Alternatively unless the throughput becomes too much, the
        // redox version of journald (to be written) would handle that.
        true
    }
    fn log(&self, record: &Record) {
        let _ = Self::write_record(false, record, &mut *self.file.lock().unwrap());
        if let Some(ref stdout) = self.stdout {
            let _ = Self::write_record(true, record, &mut stdout.lock());
        }
        if self.flush { self.flush() }
    }
    fn flush(&self) {
        let _ = self.file.lock().unwrap().flush();
    }
}
