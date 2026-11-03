//! Field addresses such as `PID-5.1`.

use std::fmt;

/// A position inside a message: `SEG-field[(repetition)][.component[.sub]]`.
/// All numbers are 1-based, as in the HL7 standard; `MSH-1` is the field
/// separator and `MSH-2` the encoding characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
    /// The segment id, e.g. `PID` (the first segment of that name is meant).
    pub segment: String,
    /// The field number (1-based).
    pub field: usize,
    /// The repetition number (1-based); `None` means the first.
    pub repetition: Option<usize>,
    /// The component number (1-based), if the path goes below the field.
    pub component: Option<usize>,
    /// The subcomponent number (1-based), if the path goes below a component.
    pub subcomponent: Option<usize>,
}

/// Why a path string could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PathError {
    /// There is no segment id before the `-`.
    MissingSegment,
    /// There is no field number after the `-`.
    MissingField,
    /// A number is absent, zero or not a number.
    BadNumber,
    /// Something follows the address that is not part of it.
    Trailing,
}

impl fmt::Display for PathError {
    /// Renders a short lowercase reason, suitable for a status-line message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PathError::MissingSegment => "missing segment id",
            PathError::MissingField => "missing field number",
            PathError::BadNumber => "bad number",
            PathError::Trailing => "unexpected trailing text",
        })
    }
}

impl std::error::Error for PathError {}

/// Parse a 1-based index; zero and non-numeric text are both [`PathError::BadNumber`].
fn number(s: &str) -> Result<usize, PathError> {
    match s.parse::<usize>() {
        Ok(n) if n > 0 => Ok(n),
        _ => Err(PathError::BadNumber),
    }
}

impl Path {
    /// A path to a whole field.
    #[must_use]
    pub fn field(segment: &str, field: usize) -> Path {
        Path {
            segment: segment.to_string(),
            field,
            repetition: None,
            component: None,
            subcomponent: None,
        }
    }

    /// Read `SEG-5`, `SEG-5.1`, `SEG-5.1.2` or `SEG-5(2).1`.
    ///
    /// # Errors
    /// Returns a [`PathError`] when the text is not a well-formed address.
    pub fn parse(text: &str) -> Result<Path, PathError> {
        let text = text.trim();
        let (segment, rest) = text.split_once('-').ok_or(PathError::MissingField)?;
        if segment.is_empty() {
            return Err(PathError::MissingSegment);
        }
        let mut parts = rest.split('.');
        let head = parts.next().unwrap_or_default();
        if head.is_empty() {
            return Err(PathError::MissingField);
        }
        let (field, repetition) = match head.split_once('(') {
            Some((f, r)) => {
                let r = r.strip_suffix(')').ok_or(PathError::Trailing)?;
                (number(f)?, Some(number(r)?))
            }
            None => (number(head)?, None),
        };
        let component = parts.next().map(number).transpose()?;
        let subcomponent = parts.next().map(number).transpose()?;
        if parts.next().is_some() {
            return Err(PathError::Trailing);
        }
        Ok(Path {
            segment: segment.to_string(),
            field,
            repetition,
            component,
            subcomponent,
        })
    }
}

impl fmt::Display for Path {
    /// Renders the canonical path text (`PID-5(2).1.3`), omitting absent parts; the output re-parses to the same [`Path`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.segment, self.field)?;
        if let Some(r) = self.repetition {
            write!(f, "({r})")?;
        }
        if let Some(c) = self.component {
            write!(f, ".{c}")?;
        }
        if let Some(s) = self.subcomponent {
            write!(f, ".{s}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints() {
        for s in ["PID-5", "PID-5.1", "PID-5.1.2", "PV1-3(2).1", "MSH-9.2"] {
            assert_eq!(Path::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn rejects_bad_paths() {
        assert_eq!(Path::parse("PID"), Err(PathError::MissingField));
        assert_eq!(Path::parse("-5"), Err(PathError::MissingSegment));
        assert_eq!(Path::parse("PID-"), Err(PathError::MissingField));
        assert_eq!(Path::parse("PID-0"), Err(PathError::BadNumber));
        assert_eq!(Path::parse("PID-x"), Err(PathError::BadNumber));
        assert_eq!(Path::parse("PID-1.2.3.4"), Err(PathError::Trailing));
        assert_eq!(Path::parse("PID-1(2"), Err(PathError::Trailing));
    }
}
