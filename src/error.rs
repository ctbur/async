use std::borrow::Cow;
use std::{fmt, io};

pub type Result<T> = std::result::Result<T, Error>;

pub fn report<T>(res: Result<T>) {
    if let Err(err) = res {
        eprintln!("{}", err);
    }
}

pub trait ResultExt<T> {
    fn with_context<S>(self, context: S) -> Result<T>
    where
        S: Into<Cow<'static, str>>;
}

impl<T, E> ResultExt<T> for std::result::Result<T, E>
where
    E: Into<Error>,
{
    fn with_context<S>(self, context: S) -> Result<T>
    where
        S: Into<Cow<'static, str>>,
    {
        self.map_err(|e| {
            let mut err = e.into();
            err.context = Some(context.into());
            return err;
        })
    }
}

#[derive(Debug)]
pub struct Error {
    cause: Option<Cause>,
    context: Option<Cow<'static, str>>,
}

impl Error {
    fn with_cause(cause: Cause) -> Self {
        Self {
            cause: Some(cause),
            context: None,
        }
    }

    pub fn with_context<S>(context: S) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        Self {
            cause: None,
            context: Some(context.into()),
        }
    }
}

#[derive(Debug)]
enum Cause {
    Io(io::Error),
    Serde(bincode::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref context) = self.context {
            writeln!(f, "Error: {}", context)?;
        } else {
            writeln!(f, "An error occurred")?;
        }

        if let Some(ref cause) = self.cause {
            write!(f, "Caused by ")?;
            match *cause {
                Cause::Io(ref err) => write!(f, "IO error: {}", err)?,
                Cause::Serde(ref err) => write!(f, "Serialization error: {}", err)?,
            };
        }

        return Ok(());
    }
}

impl std::error::Error for Error {
    fn cause(&self) -> Option<&dyn std::error::Error> {
        if let Some(ref cause) = self.cause {
            match *cause {
                Cause::Io(ref err) => Some(err),
                Cause::Serde(ref err) => Some(err),
            }
        } else {
            None
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::with_cause(Cause::Io(err))
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Error {
        Error::with_cause(Cause::Serde(err))
    }
}
