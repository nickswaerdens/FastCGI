use bytes::{BufMut, BytesMut};

use crate::{
    codec::Frame,
    record::{DecodeFrameError, RecordType},
    request, response,
};

pub type ParseResult<T> = Result<T, ParseError>;

pub(crate) trait State {
    type Transition;
    type Output;

    fn parse_transition(frame: Frame) -> ParseResult<Self::Transition>;

    fn parse_frame(&mut self, transition: Self::Transition) -> ParseResult<Option<Self::Output>>;
}

impl State for client::State {
    type Transition = client::Transition;
    type Output = response::Part;

    fn parse_transition(frame: Frame) -> ParseResult<Self::Transition> {
        Self::Transition::parse(frame)
    }

    fn parse_frame(&mut self, transition: Self::Transition) -> ParseResult<Option<Self::Output>> {
        self.parse_frame(transition)
    }
}

impl State for server::State {
    type Transition = server::Transition;
    type Output = request::Part;

    fn parse_transition(frame: Frame) -> ParseResult<Self::Transition> {
        Ok(Self::Transition::parse(frame))
    }

    fn parse_frame(&mut self, transition: Self::Transition) -> ParseResult<Option<Self::Output>> {
        self.parse_frame(transition)
    }
}

/// Temporarily stores received stream frames of the same record type.
///
/// The default maximum size of the payload is 64MB (1024 full frames). This can be adjusted
/// with `with_max_payload_size`. As the project is at an early stage, it's recommended to
/// manually set the maximum to avoid unexpected changes to the maximum payload size in the
/// future.
#[derive(Debug)]
pub(crate) struct Defrag {
    payloads: Vec<BytesMut>,
    max_total_payload: usize,
    current_total_payload: usize,
}

impl Defrag {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_max_payload_size(mut self, n: usize) -> Self {
        self.max_total_payload = n;
        self
    }

    pub(crate) fn insert_payload(&mut self, payload: BytesMut) -> ParseResult<()> {
        let n = payload.len();

        let new_size = self.current_total_payload + n;
        if self.max_total_payload < new_size {
            return Err(ParseError::ExceededMaximumStreamSize((
                new_size,
                self.max_total_payload,
            )));
        }

        self.payloads.push(payload);
        self.current_total_payload = new_size;

        Ok(())
    }

    pub(crate) fn handle_end_of_stream(&mut self) -> Option<BytesMut> {
        if self.payloads.is_empty() {
            return None;
        }

        // Should this much space be reserved beforehand?
        // The frames drain iter could be chunked, with memory being reserved for each chunk instead.
        let mut buffer = BytesMut::with_capacity(self.current_total_payload);

        for payload in self.payloads.drain(..) {
            buffer.put(payload);
        }

        Some(buffer)
    }
}

impl Default for Defrag {
    fn default() -> Self {
        Self {
            payloads: Vec::new(),
            max_total_payload: 0x4000000, // 64 MB
            current_total_payload: 0,
        }
    }
}

pub(crate) mod client {
    use bytes::BytesMut;

    use crate::{
        codec::Frame,
        record::{DecodeFrame, DecodeFrameError, EndRequest, RecordType, Standard, Stderr, Stdout},
        response::Part,
    };

    use super::{Defrag, ParseError, ParseResult};

    #[derive(Debug, Clone, Copy)]
    enum StreamState {
        Init,
        Started,
        Ended,
    }

    #[derive(Debug, Clone, Copy)]
    enum ResponseState {
        Std { out: StreamState, err: StreamState },
        Finished,
    }

    #[derive(Debug)]
    pub(crate) enum Transition {
        ParseStdout(BytesMut),
        ParseStderr(BytesMut),
        EndOfStdout,
        EndOfStderr,
        ParseEndRequest(BytesMut),
        Unsupported,
    }

    impl Transition {
        pub(crate) fn parse(frame: Frame) -> ParseResult<Transition> {
            let (header, payload) = frame.into_parts();

            if header.id == 0 {
                return Ok(Transition::Unsupported);
            }

            let transition = match (header.record_type, payload.is_empty()) {
                (RecordType::Standard(Standard::Stdout), false) => Transition::ParseStdout(payload),
                (RecordType::Standard(Standard::Stdout), true) => Transition::EndOfStdout,

                (RecordType::Standard(Standard::Stderr), false) => Transition::ParseStderr(payload),
                (RecordType::Standard(Standard::Stderr), true) => Transition::EndOfStderr,

                (RecordType::Standard(Standard::EndRequest), false) => {
                    Transition::ParseEndRequest(payload)
                }
                (RecordType::Standard(Standard::EndRequest), true) => {
                    return Err(ParseError::DecodeFrameError(
                        DecodeFrameError::InsufficientDataInBuffer,
                    ))
                }

                (record_type, _) => return Err(ParseError::UnexpectedRecordType(record_type)),
            };

            Ok(transition)
        }
    }

    #[derive(Debug)]
    pub(crate) struct State {
        inner: ResponseState,

        // stdout and stderr can be interleaved.
        stdout_defrag: Defrag,
        stderr_defrag: Defrag,
    }

    impl State {
        pub(crate) fn parse_frame(&mut self, transition: Transition) -> ParseResult<Option<Part>> {
            let record = match (self.inner, transition) {
                // Stdout
                (
                    ResponseState::Std {
                        out: StreamState::Init,
                        err,
                    },
                    Transition::ParseStdout(payload),
                ) => {
                    self.stdout_defrag.insert_payload(payload)?;

                    self.inner = ResponseState::Std {
                        out: StreamState::Started,
                        err,
                    };

                    None
                }
                (
                    ResponseState::Std {
                        out: StreamState::Started,
                        ..
                    },
                    Transition::ParseStdout(payload),
                ) => {
                    self.stdout_defrag.insert_payload(payload)?;
                    None
                }
                (
                    ResponseState::Std {
                        out: StreamState::Started,
                        err,
                    },
                    Transition::EndOfStdout,
                ) => {
                    let payload = self
                        .stdout_defrag
                        .handle_end_of_stream()
                        .map(Stdout::decode)
                        .transpose()?;

                    self.inner = ResponseState::Std {
                        out: StreamState::Ended,
                        err,
                    };

                    payload.map(Part::from)
                }

                // Stderr
                (
                    ResponseState::Std {
                        err: StreamState::Init,
                        out,
                    },
                    Transition::ParseStderr(payload),
                ) => {
                    self.stderr_defrag.insert_payload(payload)?;

                    self.inner = ResponseState::Std {
                        err: StreamState::Started,
                        out,
                    };

                    None
                }
                (
                    ResponseState::Std {
                        err: StreamState::Started,
                        ..
                    },
                    Transition::ParseStderr(payload),
                ) => {
                    self.stderr_defrag.insert_payload(payload)?;
                    None
                }

                // Optionally parse empty stderr requests, even if there was no actual stderr response.
                (
                    ResponseState::Std {
                        err: StreamState::Started | StreamState::Init,
                        out,
                    },
                    Transition::EndOfStderr,
                ) => {
                    let record = self
                        .stderr_defrag
                        .handle_end_of_stream()
                        .map(Stderr::decode)
                        .transpose()?;

                    self.inner = ResponseState::Std {
                        err: StreamState::Ended,
                        out,
                    };

                    record.map(Part::from)
                }

                // EndRequest
                (
                    ResponseState::Std {
                        out: StreamState::Ended,
                        err: StreamState::Init | StreamState::Ended,
                    },
                    Transition::ParseEndRequest(payload),
                ) => {
                    let end_request = EndRequest::decode(payload)?;

                    self.inner = ResponseState::Finished;

                    Some(end_request.into())
                }

                // Unsupported
                (_, Transition::Unsupported) => {
                    // TODO: Add Logger warning.
                    println!("Record ignored: management records are currently not supported.");
                    return Ok(None);
                }

                // Invalid state
                _ => return Err(ParseError::InvalidState),
            };

            Ok(record)
        }
    }

    impl Default for State {
        fn default() -> Self {
            Self {
                inner: ResponseState::Std {
                    out: StreamState::Init,
                    err: StreamState::Init,
                },
                stdout_defrag: Defrag::default(),
                stderr_defrag: Defrag::default(),
            }
        }
    }
}

pub(crate) mod server {
    use crate::{
        codec::Frame,
        record::{
            begin_request::Role, AbortRequest, BeginRequest, Data, DecodeFrame, Params, RecordType,
            Standard, Stdin,
        },
        request::Part,
    };

    use super::{Defrag, ParseError, ParseResult};

    fn validate_record_type(lh: RecordType, rh: impl PartialEq<RecordType>) -> ParseResult<()> {
        (rh == lh)
            .then_some(())
            .ok_or(ParseError::UnexpectedRecordType(lh))
    }

    #[derive(Debug, Clone, Copy)]
    enum RequestState {
        BeginRequest,
        Params,
        Stdin,
        Data,
        Finished,
        Aborted,
    }

    #[derive(Debug)]
    pub(crate) enum Transition {
        Parse(Frame),
        EndOfStream(RecordType),
        Abort,
        Unsupported,
    }

    impl Transition {
        pub(crate) fn parse(frame: Frame) -> Transition {
            let (header, payload) = frame.as_parts();

            if header.id == 0 {
                return Transition::Unsupported;
            }

            if !payload.is_empty() {
                Transition::Parse(frame)
            } else if header.record_type == Standard::AbortRequest {
                Transition::Abort
            } else {
                Transition::EndOfStream(header.record_type)
            }
        }
    }

    #[derive(Debug)]
    pub(crate) struct State {
        inner: RequestState,
        role: Option<Role>,
        defrag: Defrag,
    }

    impl State {
        pub(crate) fn parse_frame(&mut self, transition: Transition) -> ParseResult<Option<Part>> {
            let part = match (self.inner, transition) {
                (RequestState::BeginRequest, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::BeginRequest)?;

                    let begin_request = BeginRequest::decode(payload)?;

                    self.role = Some(begin_request.get_role());
                    self.inner = RequestState::Params;

                    Some(begin_request.into())
                }

                (RequestState::Params, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::Params)?;

                    self.defrag.insert_payload(payload)?;

                    None
                }
                (RequestState::Params, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Params)?;

                    let record = self
                        .defrag
                        .handle_end_of_stream()
                        .map(Params::decode)
                        .transpose()?;

                    self.inner = RequestState::Stdin;

                    record.map(Part::from)
                }

                (RequestState::Stdin, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::Stdin)?;

                    self.defrag.insert_payload(payload)?;

                    None
                }
                (RequestState::Stdin, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Stdin)?;

                    let record = self
                        .defrag
                        .handle_end_of_stream()
                        .map(Stdin::decode)
                        .transpose()?;

                    self.inner = match self.role.ok_or(ParseError::InvalidState)? {
                        Role::Filter => RequestState::Data,
                        _ => RequestState::Finished,
                    };

                    record.map(Part::from)
                }

                (RequestState::Data, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::Data)?;

                    self.defrag.insert_payload(payload)?;

                    None
                }
                (RequestState::Data, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Data)?;

                    let record = self
                        .defrag
                        .handle_end_of_stream()
                        .map(Data::decode)
                        .transpose()?;

                    self.inner = RequestState::Finished;

                    record.map(Part::from)
                }

                // Abort
                (
                    RequestState::Params | RequestState::Stdin | RequestState::Data,
                    Transition::Abort,
                ) => {
                    self.inner = RequestState::Aborted;

                    Some(AbortRequest.into())
                }

                // Unsupported
                (_, Transition::Unsupported) => {
                    // TODO: Add Logger warning.
                    println!("Record ignored: management records are currently not supported.");
                    return Ok(None);
                }

                // Errors
                (_, Transition::EndOfStream(_)) => return Err(ParseError::UnexpectedEndOfStream),
                (_, Transition::Abort) => return Err(ParseError::UnexpectedAbortRequest),

                _ => return Err(ParseError::InvalidState),
            };

            Ok(part)
        }
    }

    impl Default for State {
        fn default() -> Self {
            Self {
                inner: RequestState::BeginRequest,
                role: None,
                defrag: Defrag::new(),
            }
        }
    }
}

#[derive(Debug)]
pub enum ParseError {
    InvalidState,
    UnexpectedRecordType(RecordType),
    UnexpectedEndOfStream,
    UnexpectedAbortRequest,

    // Defrag
    ExceededMaximumStreamSize((usize, usize)),

    DecodeFrameError(DecodeFrameError),
    StdIoError(std::io::Error),
}

impl From<std::io::Error> for ParseError {
    fn from(value: std::io::Error) -> Self {
        ParseError::StdIoError(value)
    }
}

impl From<DecodeFrameError> for ParseError {
    fn from(value: DecodeFrameError) -> Self {
        ParseError::DecodeFrameError(value)
    }
}
