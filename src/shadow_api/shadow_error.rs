use std::{error::Error, fmt};

#[derive(Debug)]
pub struct ShadowError {
    pub(crate) msg: String,
}

impl fmt::Display for ShadowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[ShadowError] {}", self.msg)
    }
}

impl Error for ShadowError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl From<&str> for ShadowError {
    fn from(value: &str) -> Self {
        ShadowError {
            msg: value.to_owned(),
        }
    }
}

impl From<String> for ShadowError {
    fn from(msg: String) -> Self {
        ShadowError {
            msg,
        }
    }
}

impl From<std::io::Error> for ShadowError {
    fn from(msg: std::io::Error) -> Self {
        ShadowError {
            msg: msg.to_string(),
        }
    }
}