use bytes::{BufMut, BytesMut};

use crate::{codec::Frame, request, response};

pub(crate) trait State: Default {
    type Transition;
    type Output: std::fmt::Debug;
    type Error: ParseError + std::fmt::Debug;

    fn parse_transition(frame: Frame) -> Result<Self::Transition, Self::Error>;

    fn parse_frame(
        &mut self,
        transition: Self::Transition,
    ) -> Result<Option<Self::Output>, Self::Error>;
}

impl State for client::State {
    type Transition = client::Transition;
    type Output = response::Part;
    type Error = client::ParseResponseError;

    fn parse_transition(frame: Frame) -> Result<Self::Transition, Self::Error> {
        Self::Transition::parse(frame)
    }

    fn parse_frame(
        &mut self,
        transition: Self::Transition,
    ) -> Result<Option<Self::Output>, Self::Error> {
        self.parse_frame(transition)
    }
}

impl State for server::State {
    type Transition = server::Transition;
    type Output = request::Part;
    type Error = server::ParseRequestError;

    fn parse_transition(frame: Frame) -> Result<Self::Transition, Self::Error> {
        Ok(Self::Transition::parse(frame))
    }

    fn parse_frame(
        &mut self,
        transition: Self::Transition,
    ) -> Result<Option<Self::Output>, Self::Error> {
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

    pub(crate) fn insert_payload(
        &mut self,
        payload: BytesMut,
    ) -> Result<(), ExceededMaximumStreamSize> {
        let new_size = self.current_total_payload + payload.len();

        if self.max_total_payload < new_size {
            Err(ExceededMaximumStreamSize(new_size, self.max_total_payload))?;
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

pub struct ExceededMaximumStreamSize(usize, usize);

impl std::fmt::Debug for ExceededMaximumStreamSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The stream has exceeded it's maximum allowed size [{} < {}].",
            self.0, self.1
        )
    }
}

pub mod client {
    use bytes::BytesMut;

    use crate::{
        codec::Frame,
        record::{DecodeFrame, DecodeFrameError, EndRequest, RecordType, Standard, Stderr, Stdout},
        response::Part,
    };

    use super::{Defrag, ExceededMaximumStreamSize};

    type ParseResult<T> = Result<T, ParseResponseError>;

    #[derive(Debug, Default, Clone, Copy)]
    enum StreamState {
        #[default]
        Init,
        Started,
        Ended,
    }

    #[derive(Debug, Clone, Copy)]
    enum Inner {
        Std { out: StreamState, err: StreamState },
        Finished,
    }

    impl Default for Inner {
        fn default() -> Self {
            Inner::Std {
                out: StreamState::default(),
                err: StreamState::default(),
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
            let (header, payload) = frame.into_parts();

            assert!(header.id > 0);

            let transition = match (header.record_type, payload.is_empty()) {
                (RecordType::Standard(Standard::Stdout), false) => Transition::ParseStdout(payload),
                (RecordType::Standard(Standard::Stdout), true) => Transition::EndOfStdout,

                (RecordType::Standard(Standard::Stderr), false) => Transition::ParseStderr(payload),
                (RecordType::Standard(Standard::Stderr), true) => Transition::EndOfStderr,

                (RecordType::Standard(Standard::EndRequest), false) => {
                    Transition::ParseEndRequest(payload)
                }
                (RecordType::Standard(Standard::EndRequest), true) => {
                    return Err(ParseResponseError::DecodeFrameError(
                        DecodeFrameError::InsufficientDataInBuffer,
                    ))
                }

                (record_type, _) => {
                    return Err(ParseResponseError::UnexpectedRecordType(record_type))
                }
            };

            Ok(transition)
        }
    }

    #[derive(Debug, Default)]
    pub(crate) struct State {
        inner: Inner,

        // stdout and stderr can be interleaved.
        stdout_defrag: Defrag,
        stderr_defrag: Defrag,
    }

    impl State {
        pub(crate) fn new() -> Self {
            Self {
                inner: Inner::Std {
                    out: StreamState::Init,
                    err: StreamState::Init,
                },
                stdout_defrag: Defrag::default(),
                stderr_defrag: Defrag::default(),
            }
        }

        /// Return a part when it can be fully constructed, otherwise returns None.
        pub(crate) fn parse_frame(&mut self, transition: Transition) -> ParseResult<Option<Part>> {
            let record = match (self.inner, transition) {
                // Stdout
                (
                    Inner::Std {
                        out: StreamState::Init,
                        err,
                    },
                    Transition::ParseStdout(payload),
                ) => {
                    self.stdout_defrag.insert_payload(payload)?;

                    self.inner = Inner::Std {
                        out: StreamState::Started,
                        err,
                    };

                    None
                }
                (
                    Inner::Std {
                        out: StreamState::Started,
                        ..
                    },
                    Transition::ParseStdout(payload),
                ) => {
                    self.stdout_defrag.insert_payload(payload)?;
                    None
                }

                // EndOfStdout
                (
                    Inner::Std {
                        out: StreamState::Init,
                        err,
                    },
                    Transition::EndOfStdout,
                ) => {
                    self.inner = Inner::Std {
                        out: StreamState::Ended,
                        err,
                    };

                    Some(Part::Stdout(None))
                }
                (
                    Inner::Std {
                        out: StreamState::Started,
                        err,
                    },
                    Transition::EndOfStdout,
                ) => {
                    let stdout = self
                        .stdout_defrag
                        .handle_end_of_stream()
                        .map(Stdout::decode_frame)
                        .transpose()?;

                    self.inner = Inner::Std {
                        out: StreamState::Ended,
                        err,
                    };

                    Some(Part::from(stdout))
                }

                // Stderr
                (
                    Inner::Std {
                        err: StreamState::Init,
                        out,
                    },
                    Transition::ParseStderr(payload),
                ) => {
                    self.stderr_defrag.insert_payload(payload)?;

                    self.inner = Inner::Std {
                        err: StreamState::Started,
                        out,
                    };

                    None
                }
                (
                    Inner::Std {
                        err: StreamState::Started,
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
                    Inner::Std {
                        err: StreamState::Init,
                        out,
                    },
                    Transition::EndOfStderr,
                ) => {
                    self.inner = Inner::Std {
                        err: StreamState::Ended,
                        out,
                    };

                    Some(Part::Stderr(None))
                }
                (
                    Inner::Std {
                        err: StreamState::Started,
                        out,
                    },
                    Transition::EndOfStderr,
                ) => {
                    let stderr = self
                        .stderr_defrag
                        .handle_end_of_stream()
                        .map(Stderr::decode_frame)
                        .transpose()?;

                    self.inner = Inner::Std {
                        err: StreamState::Ended,
                        out,
                    };

                    Some(Part::from(stderr))
                }

                // EndRequest
                (
                    Inner::Std {
                        out: StreamState::Ended,
                        err: StreamState::Init | StreamState::Ended,
                    },
                    Transition::ParseEndRequest(payload),
                ) => {
                    let end_request = EndRequest::decode_frame(payload)?;

                    self.inner = Inner::Finished;

                    Some(Part::from(end_request))
                }

                // Invalid state
                _ => return Err(ParseResponseError::InvalidState),
            };

            Ok(record)
        }
    }

    #[derive(Debug)]
    pub enum ParseResponseError {
        InvalidState,
        UnexpectedRecordType(RecordType),

        // Defrag
        ExceededMaximumStreamSize(ExceededMaximumStreamSize),

        DecodeFrameError(DecodeFrameError),
        StdIoError(std::io::Error),
    }

    impl From<std::io::Error> for ParseResponseError {
        fn from(value: std::io::Error) -> Self {
            ParseResponseError::StdIoError(value)
        }
    }

    impl From<DecodeFrameError> for ParseResponseError {
        fn from(value: DecodeFrameError) -> Self {
            ParseResponseError::DecodeFrameError(value)
        }
    }

    impl From<ExceededMaximumStreamSize> for ParseResponseError {
        fn from(value: ExceededMaximumStreamSize) -> Self {
            ParseResponseError::ExceededMaximumStreamSize(value)
        }
    }
}

pub mod server {
    use crate::{
        codec::Frame,
        record::{
            begin_request::Role, BeginRequest, Data, DecodeFrame, DecodeFrameError, Params,
            RecordType, Standard, Stdin,
        },
        request::Part,
    };

    use super::{Defrag, ExceededMaximumStreamSize};

    type ParseResult<T> = Result<T, ParseRequestError>;

    fn validate_record_type(lh: RecordType, rh: impl PartialEq<RecordType>) -> ParseResult<()> {
        (rh == lh)
            .then_some(())
            .ok_or(ParseRequestError::UnexpectedRecordType(lh))
    }

    #[derive(Debug, Default, Clone, Copy)]
    enum Inner {
        #[default]
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
    }

    impl Transition {
        pub(crate) fn parse(frame: Frame) -> Transition {
            let (header, payload) = frame.as_parts();

            assert!(header.id > 0);

            if !payload.is_empty() {
                Transition::Parse(frame)
            } else if header.record_type == Standard::AbortRequest {
                Transition::Abort
            } else {
                Transition::EndOfStream(header.record_type)
            }
        }
    }

    #[derive(Debug, Default)]
    pub(crate) struct State {
        inner: Inner,
        role: Option<Role>,
        defrag: Defrag,
    }

    impl State {
        pub(crate) fn new() -> Self {
            State {
                inner: Inner::BeginRequest,
                role: None,
                defrag: Defrag::new(),
            }
        }

        /// Return a Part when it can be fully constructed, otherwise returns None.
        pub(crate) fn parse_frame(&mut self, transition: Transition) -> ParseResult<Option<Part>> {
            let part = match (self.inner, transition) {
                (Inner::BeginRequest, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::BeginRequest)?;

                    let begin_request = BeginRequest::decode_frame(payload)?;

                    self.role = Some(begin_request.get_role());
                    self.inner = Inner::Params;

                    Some(Part::from(begin_request))
                }

                (Inner::Params, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::Params)?;

                    self.defrag.insert_payload(payload)?;

                    None
                }
                (Inner::Params, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Params)?;

                    let params = self
                        .defrag
                        .handle_end_of_stream()
                        .map(Params::decode_frame)
                        .transpose()?;

                    self.inner = Inner::Stdin;

                    if params.is_none() {
                        return Err(ParseRequestError::ParamsMustBeLargerThanZero);
                    }

                    params.map(Part::from)
                }

                (Inner::Stdin, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::Stdin)?;

                    self.defrag.insert_payload(payload)?;

                    None
                }
                (Inner::Stdin, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Stdin)?;

                    let stdin = self
                        .defrag
                        .handle_end_of_stream()
                        .map(Stdin::decode_frame)
                        .transpose()?;

                    self.inner = match self
                        .role
                        .expect("Invalid state reached while parsing the request.")
                    {
                        Role::Filter => Inner::Data,
                        _ => Inner::Finished,
                    };

                    Some(Part::from(stdin))
                }

                (Inner::Data, Transition::Parse(frame)) => {
                    let (header, payload) = frame.into_parts();

                    validate_record_type(header.record_type, Standard::Data)?;

                    self.defrag.insert_payload(payload)?;

                    None
                }
                (Inner::Data, Transition::EndOfStream(record_type)) => {
                    validate_record_type(record_type, Standard::Data)?;

                    let data = self
                        .defrag
                        .handle_end_of_stream()
                        .map(Data::decode_frame)
                        .transpose()?;

                    self.inner = Inner::Finished;

                    if data.is_none() {
                        return Err(ParseRequestError::DataIsRequiredForFilterApplications);
                    }

                    data.map(Part::from)
                }

                // Abort
                (Inner::Params | Inner::Stdin | Inner::Data, Transition::Abort) => {
                    self.inner = Inner::Aborted;

                    Some(Part::AbortRequest)
                }

                // Errors
                (Inner::Finished, _) => return Err(ParseRequestError::InvalidState),

                (_, Transition::Abort) => return Err(ParseRequestError::UnexpectedAbortRequest),
                (_, Transition::EndOfStream(record_type)) => {
                    return Err(ParseRequestError::UnexpectedRecordType(record_type))
                }
                (_, Transition::Parse(frame)) => {
                    return Err(ParseRequestError::UnexpectedRecordType(
                        frame.header.record_type,
                    ))
                }
            };

            Ok(part)
        }
    }

    #[derive(Debug)]
    pub enum ParseRequestError {
        InvalidState,
        UnexpectedRecordType(RecordType),

        // Specific errors.
        UnexpectedAbortRequest,
        ParamsMustBeLargerThanZero,
        DataIsRequiredForFilterApplications,

        // Defrag
        ExceededMaximumStreamSize(ExceededMaximumStreamSize),

        DecodeFrameError(DecodeFrameError),
        StdIoError(std::io::Error),
    }

    impl From<std::io::Error> for ParseRequestError {
        fn from(value: std::io::Error) -> Self {
            ParseRequestError::StdIoError(value)
        }
    }

    impl From<DecodeFrameError> for ParseRequestError {
        fn from(value: DecodeFrameError) -> Self {
            ParseRequestError::DecodeFrameError(value)
        }
    }

    impl From<ExceededMaximumStreamSize> for ParseRequestError {
        fn from(value: ExceededMaximumStreamSize) -> Self {
            ParseRequestError::ExceededMaximumStreamSize(value)
        }
    }
}

pub trait ParseError {}
impl ParseError for client::ParseResponseError {}
impl ParseError for server::ParseRequestError {}
