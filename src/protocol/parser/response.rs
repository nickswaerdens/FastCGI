use super::defrag::{Defrag, MaximumStreamSizeExceeded};
use crate::{
    build_enum_with_from_impls,
    protocol::{
        record::{Decode, DecodeError, EndRequest, RecordType, Standard, Stderr, Stdout},
        transport::Frame,
    },
};
use bytes::BytesMut;

type ParseResult<T> = Result<T, ParseResponseError>;

#[derive(Debug, Default, Clone, Copy)]
enum Inner {
    #[default]
    Init,
    Started,
    Ended,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Std { out: Inner, err: Inner },
    Finished,
}

impl Default for State {
    fn default() -> Self {
        State::Std {
            out: Inner::default(),
            err: Inner::default(),
        }
    }
}

#[derive(Debug)]
pub(crate) enum Transition {
    ParseStdout(BytesMut),
    ParseStderr(BytesMut),
    EndOfStdout,
    EndOfStderr,
    ParseEndRequest(BytesMut),
}

impl Transition {
    pub(crate) fn parse(frame: Frame) -> ParseResult<Transition> {
        let (id, record_type, payload) = frame.into_parts();

        assert!(id > 0);

        let transition = match (record_type, payload.is_empty()) {
            (RecordType::Standard(Standard::Stdout), false) => Transition::ParseStdout(payload),
            (RecordType::Standard(Standard::Stdout), true) => Transition::EndOfStdout,

            (RecordType::Standard(Standard::Stderr), false) => Transition::ParseStderr(payload),
            (RecordType::Standard(Standard::Stderr), true) => Transition::EndOfStderr,

            (RecordType::Standard(Standard::EndRequest), false) => {
                Transition::ParseEndRequest(payload)
            }
            (RecordType::Standard(Standard::EndRequest), true) => {
                return Err(ParseResponseError::DecodeError(
                    DecodeError::InsufficientDataInBuffer,
                ))
            }

            (record_type, _) => return Err(ParseResponseError::UnexpectedRecordType(record_type)),
        };

        Ok(transition)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Parser {
    inner: State,

    // stdout and stderr can be interleaved.
    stdout_defrag: Defrag,
    stderr_defrag: Defrag,
}

impl Parser {
    pub(crate) fn new() -> Self {
        Self {
            inner: State::Std {
                out: Inner::Init,
                err: Inner::Init,
            },
            stdout_defrag: Defrag::default(),
            stderr_defrag: Defrag::default(),
        }
    }

    /// Returns a part when it can be fully constructed, otherwise returns None.
    pub(crate) fn parse(&mut self, transition: Transition) -> ParseResult<Option<Part>> {
        let record = match (self.inner, transition) {
            // Stdout
            (
                State::Std {
                    out: Inner::Init,
                    err,
                },
                Transition::ParseStdout(payload),
            ) => {
                self.stdout_defrag.insert_payload(payload)?;

                self.inner = State::Std {
                    out: Inner::Started,
                    err,
                };

                None
            }
            (
                State::Std {
                    out: Inner::Started,
                    ..
                },
                Transition::ParseStdout(payload),
            ) => {
                self.stdout_defrag.insert_payload(payload)?;
                None
            }

            // EndOfStdout
            (
                State::Std {
                    out: Inner::Init,
                    err,
                },
                Transition::EndOfStdout,
            ) => {
                self.inner = State::Std {
                    out: Inner::Ended,
                    err,
                };

                Some(Part::Stdout(None))
            }
            (
                State::Std {
                    out: Inner::Started,
                    err,
                },
                Transition::EndOfStdout,
            ) => {
                let stdout = self
                    .stdout_defrag
                    .handle_end_of_stream()
                    .map(Stdout::decode)
                    .transpose()?;

                self.inner = State::Std {
                    out: Inner::Ended,
                    err,
                };

                Some(Part::from(stdout))
            }

            // Stderr
            (
                State::Std {
                    err: Inner::Init,
                    out,
                },
                Transition::ParseStderr(payload),
            ) => {
                self.stderr_defrag.insert_payload(payload)?;

                self.inner = State::Std {
                    err: Inner::Started,
                    out,
                };

                None
            }
            (
                State::Std {
                    err: Inner::Started,
                    ..
                },
                Transition::ParseStderr(payload),
            ) => {
                self.stderr_defrag.insert_payload(payload)?;
                None
            }

            // EndOfStderr
            // Parse optional empty Stderr requests.
            (
                State::Std {
                    err: Inner::Init,
                    out,
                },
                Transition::EndOfStderr,
            ) => {
                self.inner = State::Std {
                    err: Inner::Ended,
                    out,
                };

                Some(Part::Stderr(None))
            }
            (
                State::Std {
                    err: Inner::Started,
                    out,
                },
                Transition::EndOfStderr,
            ) => {
                let stderr = self
                    .stderr_defrag
                    .handle_end_of_stream()
                    .map(Stderr::decode)
                    .transpose()?;

                self.inner = State::Std {
                    err: Inner::Ended,
                    out,
                };

                Some(Part::from(stderr))
            }

            // EndRequest
            (
                State::Std {
                    out: Inner::Ended,
                    err: Inner::Init | Inner::Ended,
                },
                Transition::ParseEndRequest(payload),
            ) => {
                let end_request = EndRequest::decode(payload)
                    .map_err(ParseResponseError::DecodeEndRequestError)?;

                self.inner = State::Finished;

                Some(Part::from(end_request))
            }

            // Invalid state
            _ => return Err(ParseResponseError::InvalidState),
        };

        Ok(record)
    }
}

build_enum_with_from_impls! {
    pub(crate) Part {
        Stdout(Option<Stdout>),
        Stderr(Option<Stderr>),
        EndRequest(EndRequest),
    }
}

#[derive(Debug)]
pub enum ParseResponseError {
    InvalidState,
    UnexpectedRecordType(RecordType),

    // Defrag
    MaximumStreamSizeExceeded(MaximumStreamSizeExceeded),

    DecodeError(DecodeError),
    DecodeEndRequestError(DecodeError),
    StdIoError(std::io::Error),
}

impl From<std::io::Error> for ParseResponseError {
    fn from(value: std::io::Error) -> Self {
        ParseResponseError::StdIoError(value)
    }
}

impl From<DecodeError> for ParseResponseError {
    fn from(value: DecodeError) -> Self {
        ParseResponseError::DecodeError(value)
    }
}

impl From<MaximumStreamSizeExceeded> for ParseResponseError {
    fn from(value: MaximumStreamSizeExceeded) -> Self {
        ParseResponseError::MaximumStreamSizeExceeded(value)
    }
}
