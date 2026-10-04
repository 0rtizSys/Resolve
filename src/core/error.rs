use std::fmt;

/// Input rejected before it reaches storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    EmptyName,
    NameTooLong,
    ZeroPoints,
    PointsOutOfRange,
    DurationOutOfRange,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyName => "Task name cannot be empty",
            Self::NameTooLong => "Task name is too long",
            Self::ZeroPoints => "A task must be worth at least 1 point",
            Self::PointsOutOfRange => "Points must be between -1000 and +1000",
            Self::DurationOutOfRange => "A block must last between 15 minutes and 24 hours",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ValidationError {}

/// Anything that can go wrong while changing Resolve's state.
#[derive(Debug)]
pub enum Error {
    Validation(ValidationError),
    TaskNotFound(i64),
    Storage(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(err) => err.fmt(f),
            Self::TaskNotFound(id) => write!(f, "Task {id} no longer exists"),
            Self::Storage(err) => write!(f, "Storage error: {err}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Validation(err) => Some(err),
            Self::TaskNotFound(_) => None,
            Self::Storage(err) => Some(err.as_ref()),
        }
    }
}

impl From<ValidationError> for Error {
    fn from(err: ValidationError) -> Self {
        Self::Validation(err)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
