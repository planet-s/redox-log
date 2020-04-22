use std::io::BufWriter;
use std::io::prelude::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{io, fmt, fs};

use smallvec::SmallVec;
use log::{Metadata, Record};

/// An output that will be logged to. The two major outputs for most Redox system programs are
/// usually the log file, and the global stdout.
pub struct Output {
    // the actual endpoint to write to.
    endpoint: Mutex<Box<dyn Write + Send + 'static>>,

    // useful for devices like BufWrite or BufRead. You don't want the log file to never but
    // written until the program exists.
    flush_on_newline: bool,

    // specifies the maximum log level possible
    filter: log::LevelFilter,

    // specifies whether the file should contain ASCII escape codes
    ansi: bool,
}
impl fmt::Debug for Output {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Output")
            .field("endpoint", &"opaque")
            .field("flush_on_newline", &self.flush_on_newline)
            .field("filter", &self.filter)
            .field("ansi", &self.ansi)
            .finish()
    }
}

pub struct OutputBuilder {
    endpoint: Box<dyn Write + Send + 'static>,
    flush_on_newline: Option<bool>,
    filter: Option<log::LevelFilter>,
    ansi: Option<bool>,
}
impl OutputBuilder {
    #[cfg(any(target_os = "redox", rustdoc))]
    pub fn in_redox_logging_scheme<A, B, C>(category: A, subcategory: B, logfile: C) -> Result<Self, io::Error>
    where
        A: AsRef<OsStr>,
        B: AsRef<OsStr>,
        C: AsRef<OsStr>,
    {
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

        Ok(Self::with_endpoint(File::create(path)?))
    }

    pub fn stdout() -> Self {
        Self::with_endpoint(io::stdout())
    }
    pub fn stderr() -> Self {
        Self::with_endpoint(io::stderr())
    }

    pub fn with_endpoint<T>(endpoint: T) -> Self
    where
        T: Write + Send + 'static
    {
        Self::with_dyn_endpoint(Box::new(endpoint))
    }
    pub fn with_dyn_endpoint(endpoint: Box<dyn Write + Send + 'static>) -> Self {
        Self {
            endpoint,
            flush_on_newline: None,
            filter: None,
            ansi: None,
        }
    }
    pub fn flush_on_newline(mut self, flush: bool) -> Self {
        self.flush_on_newline = Some(flush);
        self
    }
    pub fn with_filter(mut self, filter: log::LevelFilter) -> Self {
        self.filter = Some(filter);
        self
    }
    pub fn with_ansi_escape_codes(mut self) -> Self {
        self.ansi = Some(true);
        self
    }
    pub fn build(self) -> Output {
        Output {
            endpoint: Mutex::new(self.endpoint),
            filter: self.filter.unwrap_or(log::LevelFilter::Info),
            flush_on_newline: self.flush_on_newline.unwrap_or(true),
            ansi: self.ansi.unwrap_or(false),
        }
    }
}

const AVG_OUTPUTS: usize = 2;

#[derive(Debug, Default)]
pub struct RedoxLogger {
    outputs: SmallVec<[Output; AVG_OUTPUTS]>,
    min_filter: Option<log::LevelFilter>,
    max_filter: Option<log::LevelFilter>,
    max_level_in_use: Option<log::LevelFilter>,
    min_level_in_use: Option<log::LevelFilter>,
}

impl RedoxLogger {
    pub fn new() -> Self {
        Self::default()
    }
    fn adjust_output_level(max_filter: Option<log::LevelFilter>, min_filter: Option<log::LevelFilter>, max_in_use: &mut Option<log::LevelFilter>, min_in_use: &mut Option<log::LevelFilter>, output: &mut Output) {
        if let Some(max) = max_filter {
            output.filter = std::cmp::max(output.filter, max);
        }
        if let Some(min) = min_filter {
            output.filter = std::cmp::min(output.filter, min);
        }
        match max_in_use {
            &mut Some(ref mut max) => *max = std::cmp::max(output.filter, *max),
            max @ &mut None => *max = Some(output.filter),
        }
        match min_in_use {
            &mut Some(ref mut min) => *min = std::cmp::min(output.filter, *min),
            min @ &mut None => *min = Some(output.filter),
        }
    }
    pub fn with_output(mut self, mut output: Output) -> Self {
        Self::adjust_output_level(self.max_filter, self.min_filter, &mut self.max_level_in_use, &mut self.min_level_in_use, &mut output);
        self.outputs.push(output);
        self
    }
    pub fn with_min_level_override(mut self, min: log::LevelFilter) -> Self {
        self.min_filter = Some(min);
        for output in &mut self.outputs {
            Self::adjust_output_level(self.max_filter, self.min_filter, &mut self.max_level_in_use, &mut self.min_level_in_use, output);
        }
        self
    }
    pub fn with_max_level_override(mut self, max: log::LevelFilter) -> Self {
        self.max_filter = Some(max);
        for output in &mut self.outputs {
            Self::adjust_output_level(self.max_filter, self.min_filter, &mut self.max_level_in_use, &mut self.min_level_in_use, output);
        }
        self
    }
    pub fn enable(self) -> Result<&'static Self, log::SetLoggerError> {
        let leak = Box::leak(Box::new(self));
        log::set_logger(leak)?;
        if let Some(max) = leak.max_level_in_use {
            log::set_max_level(max);
        } else {
            log::set_max_level(log::LevelFilter::Off);
        }
        Ok(leak)
    }
    fn write_record<W: Write>(ansi: bool, record: &Record, writer: &mut W) -> io::Result<()> {
        use termion::{color, style};
        use log::Level;


        // TODO: Log offloading to another thread or thread pool, maybe?

        let now_local = chrono::Local::now();

        // TODO: Use colors in timezone, when colors are enabled, to e.g. gray out the timezone and
        // make the actual date more readable.
        let time = now_local.format("%Y-%m-%dT%H-%M-%S.%.3f+%:z");
        let target = record.module_path().unwrap_or(record.target());
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

        let show_lines = true;
        let line_number = if show_lines { record.line() } else { None };

        struct LineFmt(Option<u32>, bool);
        impl fmt::Display for LineFmt {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                if let Some(line) = self.0 {
                    if self.1 {
                        // ansi escape codes
                        let col = color::Fg(color::LightBlack);
                        let reset = color::Fg(color::Reset);
                        write!(f, "{col:}:{line:}{reset:}", col=col, line=line, reset=reset)
                    } else {
                        // no ansi escape codes
                        write!(f, ":{}", line)
                    }
                } else {
                    write!(f, "")
                }
            }
        }

        if ansi {
            writeln!(
                writer,
                "{time:} [{target:}{line:} {level:}] {msg:}",

                time=format_args!("{m:}{col:}{msg:}{rs:}{r:}", m=style::Italic, col=time_color, msg=time, r=reset, rs=style::Reset),
                line=&LineFmt(line_number, true),
                level=format_args!("{m:}{col:}{msg:}{rs:}{r:}", m=style::Bold, col=level_color, msg=level, r=reset, rs=style::Reset),
                target=format_args!("{col:}{msg:}{r:}", col=target_color, msg=target, r=reset),
                msg=format_args!("{m:}{col:}{msg:}{rs:}{r:}", m=message_style, col=message_color, msg=message, r=reset, rs=style::Reset),
            )
        } else {
            writeln!(
                writer,
                "{time:} [{target:}{line:} {level:}] {msg:}",
                time=time,
                level=level,
                target=target,
                line=&LineFmt(line_number, false),
                msg=message,
            )
        }
    }
}

impl log::Log for RedoxLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.max_level_in_use.map(|min| metadata.level() >= min).unwrap_or(false) && self.min_level_in_use.map(|max| metadata.level() <= max).unwrap_or(false)
    }
    fn log(&self, record: &Record) {
        for output in &self.outputs {
            let mut endpoint_guard = match output.endpoint.lock() {
                Ok(e) => e,
                // poison error
                _ => continue,
            };
            if record.metadata().level() <= output.filter {
                let _ = Self::write_record(output.ansi, record, &mut *endpoint_guard);
            }

            if output.flush_on_newline {
                let _ = endpoint_guard.flush();
            }
        }
    }
    fn flush(&self) {
        for output in &self.outputs {
            match output.endpoint.lock() {
                Ok(ref mut e) => { let _ = e.flush(); }
                _ => continue,
            }
        }
    }
}
